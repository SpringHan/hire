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

use crate::{app::App, error::{AppError, AppResult}, key_event::ShellPrefix};

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

/// Read the three config files and apply them to the running app.
///
/// This is called at startup and by the `reload_config` command, so the
/// configuration is always rebuilt from the files: everything taken from them
/// (the keymap, the bookmarks, the stored tab groups, the user settings and the
/// parsed shell prefix) is dropped first. That is what makes a reload remove an
/// entry the user deleted from a file.
///
/// Reading is never fatal. A file or an entry which cannot be read is skipped
/// while the rest of the file — and the other files — are still applied; the
/// errors are returned only after the whole configuration has been read, so the
/// caller shows them once the new configuration is in place.
pub fn load_config(app: &mut App) -> AppResult<()> {
    let (auto_path, user_path, keymap_path) = get_conf_file()?;

    load_config_from(app, auto_path, user_path, keymap_path)
}

/// Read the three config files at `auto_path`, `user_path` and `keymap_path`.
///
/// It is separate from [`load_config`] so the reading/reloading rules can be
/// tested with files in a temporary directory, without the process-wide
/// `HIRE_CONFIG_DIR` override.
fn load_config_from(
    app: &mut App,
    auto_path: String,
    user_path: String,
    keymap_path: String,
) -> AppResult<()> {
    reset_config(app);
    app.config_path = auto_path.to_owned();

    let mut errors = AppError::new();

    if let Err(err) = init_auto_config(app, auto_path) {
        errors.append_errors(err.iter());
    }

    if let Err(err) = init_user_config(app, user_path) {
        errors.append_errors(err.iter());
    }

    if let Err(err) = init_keymap(app, keymap_path) {
        errors.append_errors(err.iter());
    }

    // The prefix chord is stored as text, so it is parsed into the typed form
    // once here instead of on every key press.
    if let Some(ConfigValue::String(spec)) =
        Config::find_value(&app.config, "shell_prefix")
    {
        match ShellPrefix::parse(spec) {
            Ok(prefix) => app.shell_prefix = prefix,
            Err(err) => errors.add_error(err),
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

/// Drop the configuration which [`load_config`] rebuilds from the files.
///
/// Only state coming from the files is cleared; everything else on `App`
/// (working directory, open tabs and shells, marked files, …) is untouched, so
/// reloading never changes what the user is doing.
fn reset_config(app: &mut App) {
    app.config.clear();
    app.target_dir.clear();
    app.tab_list.clear_storage();
    app.keymap = Keymap::default();
    app.shell_prefix = ShellPrefix::default();
}

fn init_auto_config(app: &mut App, path: String) -> AppResult<()> {
    let mut errors = AppError::new();
    let document: DocumentMut = get_document(path)?;

    if let Err(err) = crate::key_event::goto_read_config(app, &document) {
        errors.append_errors(err.iter());
    }

    if let Err(err) = crate::key_event::tab_read_config(app, &document) {
        errors.append_errors(err.iter());
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

fn init_user_config(app: &mut App, path: String) -> AppResult<()> {
    let configs = [
        "default_shell", "gui_commands", "file_read_program",
        "gui_file_open_cmd", "shell_prefix"
    ];
    let mut errors = AppError::new();

    // A file which cannot be read still leaves every setting at its default,
    // so `Config::get_value` never misses a name while hire is running.
    let document = match get_document(path) {
        Ok(document) => Some(document),
        Err(err) => {
            errors.add_error(err);
            None
        },
    };

    for conf in configs.into_iter() {
        let mut setting = Config::generate_default(conf);

        if let Some(document) = &document
            && let Some(item) = document.get(conf)
        {
            // A broken value is skipped: the setting keeps its default and the
            // other settings are still read.
            if let Err(err) = setting.value_from(item) {
                errors.add_error(err);
            }
        }

        app.config.push(setting);
    }

    if let Some(document) = &document
        && document.len() > configs.len()
    {
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

    // NOTE(dev): this block is a deliberate developer toggle, not a bug.
    //
    // - Production names: auto_config.toml / user_config.toml / keymap.toml
    // - Dev names:        auto_config_dev.toml / user_config_dev.toml /
    //                     keymap_dev.toml
    //
    // The dev names are enabled temporarily, in the local working tree, to try
    // a new feature without polluting the real configuration. The committed
    // state uses the production names; a working tree with the dev block
    // active is expected during development. Scripts, tests and e2e harnesses
    // must check which block is active and create the matching files (see
    // docs/ARCHITECTURE.md §6, "Dev config filenames").
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

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::command::AppCommand;

    fn temp_config_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-config-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn config_string<'a>(app: &'a App, name: &str) -> &'a str {
        match Config::find_value(&app.config, name) {
            Some(ConfigValue::String(value)) => value.as_ref(),
            _ => panic!("missing string config `{name}`"),
        }
    }

    #[test]
    fn test_load_config_applies_the_readable_entries() {
        let dir = temp_config_dir();
        let auto = write(
            &dir,
            "auto_config.toml",
            r#"
goto_dir = { h = "/home", toolong = "/tmp", d = 3 }
"#,
        );
        let user = write(
            &dir,
            "user_config.toml",
            r#"
default_shell = "zsh"
file_read_program = 12
gui_commands = ["lazygit", 3]
shell_prefix = "C-x"
"#,
        );
        let keymap = write(
            &dir,
            "keymap.toml",
            r#"
keymap = [
    { key = "x", run = "refresh" },
    { key = "y", run = "no_such_command" },
    { key = "", run = "refresh" },
]
"#,
        );

        let mut app = App::default();
        let err = load_config_from(&mut app, auto, user, keymap).unwrap_err();
        let message = err.to_string();

        // The readable user settings are applied, the broken one keeps its
        // default instead of taking the others down.
        assert_eq!(config_string(&app, "default_shell"), "zsh");
        assert_eq!(config_string(&app, "file_read_program"), "vim");

        // A broken list keeps its default as a whole (no partial commands).
        match Config::find_value(&app.config, "gui_commands") {
            Some(ConfigValue::Vec(commands)) => assert!(commands.is_empty()),
            _ => panic!("missing gui_commands"),
        }

        let prefix = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert!(app.shell_prefix.matches(&prefix));

        // Only the readable bookmark is stored.
        assert_eq!(app.target_dir.get(&'h').map(String::as_str), Some("/home"));
        assert_eq!(app.target_dir.len(), 1);

        // Only the readable binding is stored.
        assert!(matches!(app.keymap.get('x'), Ok(AppCommand::Refresh)));
        assert!(app.keymap.get('y').is_err());

        // Every error is reported after the whole configuration was read.
        assert!(message.contains("goto_dir"), "{message}");
        assert!(message.contains("file_read_program"), "{message}");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_load_config_drops_entries_removed_from_the_files() {
        let dir = temp_config_dir();
        let auto = write(&dir, "auto_config.toml", r#"goto_dir = { h = "/home" }"#);
        let user = write(&dir, "user_config.toml", r#"default_shell = "zsh""#);
        let keymap = write(
            &dir,
            "keymap.toml",
            r#"keymap = [{ key = "x", run = "refresh" }]"#,
        );

        let mut app = App::default();
        load_config_from(&mut app, auto, user, keymap).unwrap();

        assert_eq!(config_string(&app, "default_shell"), "zsh");
        assert!(app.target_dir.contains_key(&'h'));
        assert!(app.keymap.get('x').is_ok());

        // The user removes the bookmark and the binding, and changes a value.
        let auto = write(&dir, "auto_config.toml", "");
        let user = write(&dir, "user_config.toml", r#"default_shell = "bash""#);
        let keymap = write(
            &dir,
            "keymap.toml",
            r#"keymap = [{ key = "z", run = "refresh" }]"#,
        );

        load_config_from(&mut app, auto, user, keymap).unwrap();

        assert!(app.target_dir.is_empty());
        assert!(app.keymap.get('x').is_err());
        assert!(matches!(app.keymap.get('z'), Ok(AppCommand::Refresh)));
        assert_eq!(config_string(&app, "default_shell"), "bash");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_a_user_config_which_cannot_be_read_keeps_the_defaults() {
        let dir = temp_config_dir();
        let auto = write(&dir, "auto_config.toml", "");
        let user = write(&dir, "user_config.toml", "not = [ valid toml");
        let keymap = write(
            &dir,
            "keymap.toml",
            r#"keymap = [{ key = "x", run = "refresh" }]"#,
        );

        let mut app = App::default();
        assert!(load_config_from(&mut app, auto, user, keymap).is_err());

        // `Config::get_value` must never miss a name: a shell spawn would panic.
        assert_eq!(config_string(&app, "default_shell"), "bash");
        assert_eq!(config_string(&app, "file_read_program"), "vim");
        assert!(matches!(app.keymap.get('x'), Ok(AppCommand::Refresh)));

        fs::remove_dir_all(dir).unwrap();
    }
}
