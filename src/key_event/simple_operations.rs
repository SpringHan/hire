// Simple operations.

use std::io::Write;
use std::fs::OpenOptions;

use crate::utils::CmdContent;
use crate::error::{AppResult, ErrorType};

use super::App;
use super::switch::{SwitchCase, SwitchCaseData};

pub fn print_full_path(app: &mut App) {
    let file_name = if let Some(file_saver) = app.get_file_saver() {
        file_saver.name.to_owned()
    } else {
        String::new()
    };

    let mut full_path: String = app.path.to_string_lossy().into();

    full_path = if full_path == "/" {
        format!("/{}", file_name)
    } else {
        format!("{}/{}", full_path, file_name)
    };

    SwitchCase::new(
        app,
        |_, _,_| Ok(true),
        true,
        CmdContent::String(full_path),
        SwitchCaseData::None
    )
}

pub fn output_path(app: &mut App, file_out: bool) -> AppResult<()> {
    let output = if file_out {
        if let Some(file) = app.get_file_saver() {
            app.current_path().join(&file.name)
        } else {
            return Err(ErrorType::NoSelected.pack())
        }
    } else {
        app.current_path()
    };

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&app.output_file)?;

    file.write(output.to_string_lossy().as_bytes())?;

    if app.quit_after_output {
        app.quit_now = true;
    }

    Ok(())
}
