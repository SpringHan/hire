// Interaction with other Terminal tools.

use std::io::Stderr;

use ratatui::{prelude::CrosstermBackend, Terminal as RTerminal};

use super::{shell::fetch_output, CommandStr, ShellCommand};
use crate::{app::App, error::AppResult, rt_error};

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

pub fn fzf_jump(app: &mut App, terminal: &mut Terminal) -> AppResult<()> {
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

    let mut full_path = app.path.to_owned().join(target);
    if full_path.is_file() {
        if let Some(parent) = full_path.parent() {
            full_path = parent.to_path_buf();
        } else {
            rt_error!("The file path from fzf is error")
        }
    }

    app.goto_dir(full_path, None)?;

    Ok(())
}
