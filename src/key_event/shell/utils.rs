// Shell Command.

use std::io::stderr;
use std::process::Command;
use std::io::{self, Stderr};
use std::path::{Path, PathBuf};


use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, poll, Event, KeyEventKind};
use ratatui::crossterm::{
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen,
        enable_raw_mode, disable_raw_mode
    },
    cursor::{Show, Hide},
    execute
};

use crate::rt_error;
use crate::{app::App, error::AppResult};
use crate::config::{Config, ConfigValue};

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

pub enum ShellCommand<'a> {
    Shell,
    /// The first element is the shell that command runs on;
    /// The second one is the program & its arguments.
    Command(
        Option<&'a str>,
        Vec<&'a str>
    )
}

/// Start a shell process.
pub fn shell_process(app: &mut App,
                     terminal: &mut Terminal,
                     command: ShellCommand,
                     refresh: bool
) -> AppResult<()>
{
    let shell_program = if let ConfigValue::String(
        _shell
    ) = Config::get_value(&app.config, "default_shell")
    {
        _shell.as_ref().to_owned()
    } else {
        std::env::var("SHELL")?
    };

    let mut wait_for_press = false;
    let mut process = Command::new(&shell_program);

    if let ShellCommand::Command(shell_type, ref args) = command {
        let program = args[0];

        if let Some(_type) = shell_type {
            // Change shell program
            if _type != &shell_program {
                process = Command::new(_type)
            }
        }

        // Add process arguments
        process.arg("-c").arg(args.join(" "));

        // Check whether current command needs to wait for user's key press
        wait_for_press = true;

        if let ConfigValue::Vec(cmds) = Config::get_value(&app.config, "gui_commands") {
            for cmd in cmds.iter() {
                if *cmd == program {
                    wait_for_press = false;
                    break;
                }
            }
        }
    }

    process.current_dir(&app.path);


    disable_raw_mode()?;
    execute!(stderr(), LeaveAlternateScreen, Show)?;

    process.spawn()?.wait()?;

    // Wait for user press
    if wait_for_press {
        println!("Press any key to continue");
        loop {
            if poll(std::time::Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        break;
                    }
                }
            }
        }
    }

    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen, Hide)?;
    terminal.clear()?;

    if refresh {
        app.goto_dir(app.current_path(), None)?;
    }

    Ok(())
}

pub fn open_file_in_shell<P>(app: &mut App,
                         terminal: &mut Terminal,
                         file: P
) -> AppResult<()>
where P: AsRef<Path>
{
    let file_path = file.as_ref();
    let file_type = file_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let shell_command = match file_type {
        "jpg" | "jpge" | "png" => "feh",
        _ => "tetor"
    };

    shell_process(
        app,
        terminal,
        ShellCommand::Command(
            None,
            vec![shell_command, file_path.to_str().unwrap()]
        ),
        false
    )?;

    Ok(())
}

pub fn fetch_working_directory() -> AppResult<PathBuf> {
    use io::Read;

    let mut working_dir_file = std::fs::File::open(
        working_dir_cache_path()?
    )?;
    let mut working_dir = String::new();
    working_dir_file.read_to_string(&mut working_dir)?;

    if working_dir.ends_with("/") {
        working_dir = working_dir.strip_suffix("/").unwrap().to_owned();
    }

    Ok(PathBuf::from(working_dir))
}

pub fn set_working_directory<P>(path: P) -> AppResult<()>
where P: AsRef<Path>
{
    use std::io::Write;

    let working_dir_file = working_dir_cache_path()?;

    let mut working_dir_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(working_dir_file)?;

    working_dir_file.write(path.as_ref().to_string_lossy().as_bytes())?;

    Ok(())
}

fn working_dir_cache_path() -> AppResult<String> {
    match std::env::var("USER") {
        Ok(user_name) => {
            if &user_name == "root" {
                Ok(String::from("/root/.cache/st-working-directory"))
            } else {
                Ok(format!("/home/{}/.cache/st-working-directory", user_name))
            }
        },
        Err(err) => {
            rt_error!("Cannot get current user as: {err}")
        }
    }
}
