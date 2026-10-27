// Config

mod types;
mod keymap;

use std::{
    sync::OnceLock,
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Read, Write},
};

use anyhow::Result;
use toml_edit::DocumentMut;

use keymap::init_keymap;

use crate::{app::App, error::{AppError, AppResult}};

pub use types::*;
pub use keymap::Keymap;

/// A config directory used instead of the default one.
///
/// It's set by the hidden `--config-dir` argument, which a sudo session uses to
/// run with a throw-away copy of the user's config.
static CONFIG_DIR_OVERRIDE: OnceLock<String> = OnceLock::new();

/// Use `dir` as the config directory (overrides `HIRE_CONFIG_DIR`).
pub fn set_config_dir_override(dir: &str) {
    let _ = CONFIG_DIR_OVERRIDE.set(dir.to_owned());
}

/// Get T from Option<T>.
/// When it succeeded, return the value, otherwise throw anyhow error.
#[macro_export]
macro_rules! option_get {
    ($e: expr, $msg: expr) => {
        match $e {
            Some(value) => value,
            None => return Err(anyhow::anyhow!("{}", $msg).into()),
        }
    };
}

/// Pass the config file path & concrete config into App.
pub fn init_config(app: &mut App) -> AppResult<()> {
    let mut errors = AppError::new();
    let (auto_path, user_path, keymap_path) = get_conf_file()?;
    app.config_path = auto_path.to_owned();

    if let Err(err) = init_auto_config(app, auto_path) {
        errors.append_errors(err.iter());
    }

    if let Err(err) = init_user_config(app, user_path) {
        errors.append_errors(err.iter());
    }

    if let Err(err) = init_keymap(app, keymap_path) {
        errors.append_errors(err.iter());
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

fn init_auto_config(app: &mut App, path: String) -> AppResult<()> {
    let mut errors = AppError::new();
    let document: DocumentMut = get_document(path)?;

    if let Err(err) = crate::key_event::goto_read_config(app, &document) {
        errors.add_error(err);
    }

    if let Err(err) = crate::key_event::tab_read_config(app, &document) {
        errors.add_error(err);
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

fn init_user_config(app: &mut App, path: String) -> AppResult<()> {
    let configs = [
        "default_shell", "gui_commands", "file_read_program",
        "gui_file_open_cmd"
    ];
    let mut errors = AppError::new();

    let document: DocumentMut = get_document(path)?;

    for conf in configs.into_iter() {
        let mut default = Config::generate_default(conf);
        if !document.is_empty() {
            if let Some(item) = document.get(conf) {
                if let Err(err) = default.value_from(item) {
                    errors.add_error(err);
                }
            }
        }

        app.config.push(default);
    }

    if document.len() > configs.len() {
        errors.add_error(anyhow::anyhow!(
            "There're useless config in user_config.toml"
        ));
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

/// Write modified document into auto_config file.
pub fn write_document(document: DocumentMut) -> io::Result<()> {
    let (path, _, _) = get_conf_file()?;

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;

    file.write_all(document.to_string().as_bytes())?;

    Ok(())
}

pub fn get_document(path: String) -> Result<DocumentMut> {
    match File::open(&path) {
        Ok(mut file) => {
            let mut content = String::new();
            file.read_to_string(&mut content)?;

            Ok(content.parse()?)
        },
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                File::create(&path)?;
                Ok(DocumentMut::new())
            } else {
                return Err(err.into())
            }
        },
    }
}

/// Get the three config file paths, creating the config directory if needed.
///
/// The directory is `~/.config/springhan/hire/` by default. It can be
/// overridden with the `HIRE_CONFIG_DIR` environment variable, which is useful
/// for tests and for running the app with an isolated configuration (the
/// configured GUI command, for example, is then never the one of the user).
pub fn get_conf_file() -> io::Result<(String, String, String)> {
    let config_dir = if let Some(dir) = CONFIG_DIR_OVERRIDE.get() {
        format!("{}/", dir.trim().trim_end_matches('/'))
    } else {
        match std::env::var("HIRE_CONFIG_DIR") {
            Ok(dir) if !dir.trim().is_empty() => {
                format!("{}/", dir.trim().trim_end_matches('/'))
            },
            _ => {
                let user = std::env::var("USER").expect("Failed to get user name!");
                format!(
                    "{}/.config/springhan/hire/",
                    if user == String::from("root") {
                        String::from("/root")
                    } else {
                        format!("/home/{}", user)
                    }
                )
            }
        }
    };

    if let Err(err) = File::open(&config_dir) {
        if err.kind() == ErrorKind::NotFound {
            fs::create_dir_all(&config_dir)?;
        } else {
            return Err(err)
        }
    }

    Ok((
        format!("{}auto_config.toml", config_dir),
        format!("{}user_config.toml", config_dir),
        format!("{}keymap.toml", config_dir)
        // Dev
        // format!("{}auto_config_dev.toml", config_dir),
        // format!("{}user_config_dev.toml", config_dir),
        // format!("{}keymap_dev.toml", config_dir)
    ))
}
