// Shell Command.

use crate::app::{App, AppResult, ErrorType};

use std::path::{Path, PathBuf};
use std::io::{self, Stderr};

use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

pub enum ShellCommand<'a> {
    Shell,
    Command(&'a str, Option<&'a str>)
}

/// Start a shell process.
pub fn shell_process(app: &mut App,
                     terminal: &mut Terminal,
                     command: ShellCommand,
                     refresh: bool
) -> io::Result<()>
{
    use std::process::Command;
    use std::io::stderr;

    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen,
        enable_raw_mode, disable_raw_mode
    };
    use crossterm::cursor::{Show, Hide};
    use crossterm::execute;

    disable_raw_mode()?;
    execute!(stderr(), LeaveAlternateScreen, Show)?;


    let mut command_arg: Option<&str> = None;

    let command = match command {
        ShellCommand::Shell => {
            std::env::var("SHELL")
                .expect("Unable to get current command.")
        },
        ShellCommand::Command(c, arg) => {
            if let Some(arg) = arg {
                command_arg = Some(arg);
            }
            c.to_owned()
        }
    };

    let mut process = Command::new(command);
    process.current_dir(&app.path);

    if let Some(arg) = command_arg {
        process.arg(arg);
    }
    process.spawn()?.wait()?;


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
) -> io::Result<()>
where P: AsRef<Path>
{
    let file_path = file.as_ref();
    let file_type = file_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let shell_command = match file_type {
        "jpg" | "jpge" | "png" => "feh",
        _ => "bat"
    };

    shell_process(
        app,
        terminal,
        ShellCommand::Command(
            shell_command,
            Some(file_path.to_str().unwrap()),
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
            Err(
                ErrorType::Specific(
                    format!("Cannot get current user as: {}", err)
                ).pack()
            )
        }
    }
}
