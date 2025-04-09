// Interaction with other Terminal tools.

use ratatui::DefaultTerminal;

use super::{shell::fetch_output, CommandStr, ShellCommand};
use crate::{app::App, error::AppResult, option_get, rt_error};

pub fn fzf_jump(
    app: &mut App,
    terminal: &mut DefaultTerminal
) -> AppResult<()>
{
    let mut target = fetch_output(
        terminal,
        &app.path,
        ShellCommand::Command(
            None,
            CommandStr::from_strs(vec!["fzf"])
        )
    )?;

    if target.is_empty() {
        return Ok(())
    }

    if target.ends_with("\n") {
        target.pop();
    }

    let mut file_name: Option<String> = None;
    let mut full_path = app.path.to_owned().join(target);
    if full_path.is_file() {
        file_name = option_get!(full_path.file_name(), "Cannot find target file")
            .to_os_string()
            .into_string()
            .ok();

        if let Some(parent) = full_path.parent() {
            full_path = parent.to_path_buf();
        } else {
            rt_error!("The file path from fzf is error")
        }
    }

    app.goto_dir(full_path, None)?;

    if let Some(name) = file_name {
        app.file_search_sync(name, true)?;
    }

    Ok(())
}
