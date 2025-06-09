// Shell Command.

use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ratatui::DefaultTerminal;
use ratatui::crossterm::{
    execute,
    cursor::{Show, Hide},
    event::{self, poll, Event, KeyEventKind},
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen,
        enable_raw_mode, disable_raw_mode
    }
};

use crate::rt_error;
use crate::{app::App, error::AppResult};
use crate::config::{Config, ConfigValue};

use super::{CommandStr, ShellCommand};

/// Start a shell process.
pub fn shell_process(
    app: &mut App,
    terminal: &mut DefaultTerminal,
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

    // For restore the original state
    let current_file = if let Some(file) = app.get_file_saver() {
        Some(file.name.to_owned())
    } else {
        None
    };

    let mut wait_for_press = false;
    let mut process = Command::new(&shell_program);

    if let ShellCommand::Command(shell_type, args) = command {
        let program: &str = args[0].into();

        if let Some(_type) = shell_type {
            // Change shell program
            if _type != &shell_program {
                process = Command::new(_type)
            }
        }

        // Add process arguments
        process.arg("-c").arg(
            CommandStr::join_from_keymap(args, app)?
        );

        // Check whether current command needs to wait for user's key press
        wait_for_press = true;

        if let ConfigValue::Vec(
            cmds
        ) = Config::get_value(&app.config, "gui_commands")
        {
            for cmd in cmds.iter() {
                if *cmd == program {
                    wait_for_press = false;
                    break;
                }
            }
        }
    }


    // Preparation for running process
    process.current_dir(&app.path);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, Show)?;

    process.spawn()?.wait()?;

    enable_raw_mode()?;

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

    execute!(stdout(), EnterAlternateScreen, Hide)?;
    terminal.clear()?;

    if refresh {
        app.goto_dir(app.current_path(), Some(app.hide_files))?;

        if let Some(name) = current_file {
            app.file_search_sync(name, true)?;
        }
    }

    Ok(())
}

/// Run `command` & get its output.
pub fn fetch_output<P: AsRef<Path>>(
    terminal: &mut DefaultTerminal,
    current_path: P,
    command: ShellCommand
) -> anyhow::Result<String>
{
    if let ShellCommand::Command(_, mut args) = command {
        let program: &str = args.remove(0).into();
        let mut _command = Command::new(program);

        _command.current_dir(current_path).stdout(Stdio::piped());
        _command.args(CommandStr::str_vec(args));


        disable_raw_mode()?;
        execute!(stdout(), LeaveAlternateScreen, Show)?;

        let output = _command.spawn()?.wait_with_output()?;

        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide)?;
        terminal.clear()?;

        return Ok(String::from_utf8(output.stdout)?)
    }

    rt_error!("Cannot get the program that needs a output")
}

pub fn open_file_in_shell<P>(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    file: P
) -> AppResult<()>
where P: AsRef<Path>
{
    let file_path = file.as_ref();
    let file_type = file_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let mut refresh = false;
    let shell_command = match file_type {
        "jpg" | "jpge" | "png" => String::from("feh"),
        _ => {
            refresh = true;

            if let ConfigValue::String(_str) = Config::get_value(
                &app.config,
                "file_read_program"
            )
            {
                _str.as_ref().to_owned()
            } else {
                panic!("Unknown error occurred at open_file_in_shell fn in utils.rs!")
            }
        }
    };

    shell_process(
        app,
        terminal,
        ShellCommand::Command(
            None,
            vec![
                CommandStr::Str(&shell_command),
                CommandStr::Str(file_path.to_str().unwrap())
            ]
        ),
        refresh
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
