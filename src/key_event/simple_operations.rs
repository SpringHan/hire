// Simple operations.

use std::io::{Read, Write};
use std::fs::OpenOptions;

use crate::rt_error;
use crate::utils::CmdContent;
use crate::config::{Config, ConfigValue};
use crate::error::{AppResult, ErrorType};

use super::App;
use super::shell::execute_command;
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

    // Open the path with the GUI command when it has been configured.
    if let ConfigValue::String(gui_cmd) = Config::get_value(
        &app.config,
        "gui_file_open_cmd"
    ) {
        if !gui_cmd.is_empty() {
            execute_command(gui_cmd.as_ref(), &output)?;

            if app.quit_after_output {
                app.quit_now = true;
            }

            return Ok(())
        }
    }

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

    // Paths copied from a shell often keep a trailing separator (`/home/`).
    // `app.path` is shown as `<path>/<file>` in the title and by
    // `print_full_path`, so store it without one; a lone "/" (or "///") is
    // still the root.
    let target_path = normalize_target(&target_path);

    app.record_path_history();
    app.goto_dir(target_path, None)?;
    Ok(())
}

/// Remove the trailing separators from a directory path read from the temp
/// file, keeping `/` for the root.
fn normalize_target(target: &str) -> &str {
    let target = target.trim().trim_end_matches('/');

    if target.is_empty() {
        "/"
    } else {
        target
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-jump-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_normalize_target() {
        assert_eq!(normalize_target("/home/"), "/home");
        assert_eq!(normalize_target("/home///"), "/home");
        assert_eq!(normalize_target("/"), "/");
        assert_eq!(normalize_target("///"), "/");
        assert_eq!(normalize_target("relative/"), "relative");
        assert_eq!(normalize_target("  /home/ \n"), "/home");
    }

    #[test]
    fn test_jump_to_temp_file_strips_trailing_separators() {
        let dir = temp_dir("trailing");
        let temp = dir.join("hire.tmp");
        fs::write(&temp, format!("{}///\n", dir.to_string_lossy())).unwrap();

        let mut app = App {
            temp_file: temp.to_string_lossy().into_owned(),
            ..Default::default()
        };

        jump_to_temp_file(&mut app).unwrap();

        assert_eq!(app.path.to_string_lossy(), dir.to_string_lossy());
        // The target is consumed, as before.
        assert_eq!(fs::read_to_string(&temp).unwrap(), "");
    }
}
