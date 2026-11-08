// Shell Command.

use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ratatui::DefaultTerminal;
use ratatui::crossterm::{
    execute,
    cursor::{Show, Hide, MoveTo},
    event::{self, poll, Event, KeyEventKind},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
        enable_raw_mode, disable_raw_mode
    }
};

use crate::rt_error;
use crate::{app::App, error::AppResult};
use crate::config::{Config, ConfigValue};
use crate::utils::is_text_file;

use super::{CommandStr, ShellCommand};

/// The shell program used for new shell sessions and one-shot shells.
pub fn default_shell(app: &App) -> AppResult<String> {
    if let ConfigValue::String(shell) =
        Config::get_value(&app.config, "default_shell")
    {
        Ok(shell.as_ref().to_owned())
    } else {
        Ok(std::env::var("SHELL")?)
    }
}

/// Start a shell process.
pub fn shell_process(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    command: ShellCommand,
    refresh: bool
) -> AppResult<()>
{
    let shell_program = default_shell(app)?;

    // For restore the original state
    let current_file = if let Some(file) = app.get_file_saver() {
        Some(file.name.to_owned())
    } else {
        None
    };

    let mut wait_for_press = true;
    let mut process = Command::new(&shell_program);

    let ShellCommand::Command(shell_type, args) = command;
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

    // The command ran on the real screen and its output is still there. Clear
    // it and put the cursor back at the top-left, so the next one-shot command
    // starts on a clean screen instead of below this one's output. Done only
    // now, after the optional pause, so the output stays readable while it runs.
    execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))?;

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
    let ShellCommand::Command(_, mut args) = command;
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

    Ok(String::from_utf8(output.stdout)?)
}

/// Run a shell command without taking the output of it.
///
/// The command is split into the program and its arguments by whitespace,
/// and `target` is appended as the last argument of it.
///
/// The command inherits hire's terminal, so it may paint over the UI (a GUI
/// launcher usually prints nothing, but it is not guaranteed). The screen is
/// cleared once the command exits: the residue is removed and the reset buffer
/// makes the next draw repaint the whole UI.
pub fn execute_command<P: AsRef<Path>>(
    terminal: &mut DefaultTerminal,
    command: &str,
    target: P
) -> AppResult<()>
{
    let mut command_slice = command.split_whitespace();

    let program = match command_slice.next() {
        Some(program) => program,
        None => rt_error!("Cannot execute an empty command"),
    };

    Command::new(program)
        .args(command_slice)
        .arg(target.as_ref())
        .spawn()?
        .wait()?;

    terminal.clear()?;

    Ok(())
}

pub fn open_file_in_shell<P>(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    file: P
) -> AppResult<()>
where P: AsRef<Path>
{
    let file_path = file.as_ref();

    // A text file goes to the configured editor. Anything else (pictures,
    // PDFs, archives, …) is handed to the desktop's default handler, which is
    // what the preview shows for it.
    let (program, refresh) = if is_text_file(file_path) {
        if let ConfigValue::String(_str) = Config::get_value(
            &app.config,
            "file_read_program"
        ) {
            (_str.as_ref().to_owned(), true)
        } else {
            panic!("Unknown error occurred at open_file_in_shell fn in utils.rs!")
        }
    } else {
        (String::from("xdg-open"), false)
    };

    shell_process(
        app,
        terminal,
        ShellCommand::Command(
            None,
            vec![
                CommandStr::Str(&program),
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

    Ok(normalize_working_dir(&working_dir).unwrap_or_default())
}

/// Normalize the raw content read from the working-directory cache file.
///
/// `None` is returned when the content is empty. The root directory `/` is
/// kept as it is, and other trailing slashes are removed.
fn normalize_working_dir(content: &str) -> Option<PathBuf> {
    let mut working_dir = content.trim().to_owned();

    if working_dir.is_empty() {
        return None
    }

    while working_dir.len() > 1 && working_dir.ends_with('/') {
        working_dir.pop();
    }

    Some(PathBuf::from(working_dir))
}

pub fn set_working_directory<P>(
    app: &mut App,
    path: P
) -> AppResult<()>
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
    app.working_directory = Some(path.as_ref().to_owned());

    Ok(())
}

impl App<'_> {
    /// Load the working directory that has been set into `App`.
    ///
    /// Errors, such as a missing cache file, are ignored so that the app
    /// merely treats the working directory as unset.
    pub fn load_working_directory(&mut self) {
        self.working_directory = match fetch_working_directory() {
            Ok(path) if !path.as_os_str().is_empty() => Some(path),
            _ => None,
        };
    }
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_normalize_working_dir() {
        assert_eq!(
            normalize_working_dir("  /home/user/project/ \n"),
            Some(PathBuf::from("/home/user/project"))
        );
        assert_eq!(
            normalize_working_dir("/"),
            Some(PathBuf::from("/"))
        );
        assert_eq!(normalize_working_dir(""), None);
        assert_eq!(normalize_working_dir("   \n"), None);
    }
}
