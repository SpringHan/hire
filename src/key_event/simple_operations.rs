// Simple operations.

use std::io::Write;
use std::fs::OpenOptions;

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
        full_path,
        SwitchCaseData::None
    )
}

pub fn output_path(app: &mut App, file_out: bool) -> AppResult<()> {
    // NOTE: There's no possibility that the output_file is none.
    let output_path = app.output_file
        .to_owned()
        .expect("Unknow error occurred at output_path in simple_operations.rs!");

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
        .write(true)
        .truncate(true)
        .open(output_path)?;

    file.write(output.to_string_lossy().as_bytes())?;

    app.quit_now = true;

    Ok(())
}
