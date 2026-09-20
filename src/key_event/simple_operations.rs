// Simple operations.

use std::io::{Read, Write};
use std::fs::OpenOptions;

use crate::rt_error;
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
        .open(&app.temp_file)?;

    file.write(output.to_string_lossy().as_bytes())?;

    if app.quit_after_output {
        app.quit_now = true;
    }

    Ok(())
}

pub fn jump_to_temp_file(app: &mut App) -> AppResult<()> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&app.temp_file);

    if let Err(err) = file {
        if let std::io::ErrorKind::NotFound = err.kind() {
            rt_error!("The temp file is not exists!")
        } else {
            return Err(err.into())
        }
    }

    let mut file = file.unwrap();
    let mut target_path = String::new();
    file.read_to_string(&mut target_path)?;
    file.set_len(0)?;

    if target_path.trim().is_empty() {
        return Ok(())
    }

    app.goto_dir(target_path.trim(), None)?;
    Ok(())
}
