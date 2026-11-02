// Keymap

use std::collections::HashMap;

use anyhow::anyhow;
use ratatui::crossterm::event::KeyEvent;

use crate::option_get;
use crate::{
    command::AppCommand,
    error::{AppError, AppResult},
    key_event::{KeySpec, ScrollCommand},
    app::App,
};

use super::get_document;

/// One shell scroll binding: a key spec and what it does.
pub type ScrollBinding = (KeySpec, ScrollCommand);

pub struct Keymap {
    navi_maps: HashMap<char, AppCommand>,
    edit_maps: HashMap<char, AppCommand>,
    normal_maps: HashMap<char, AppCommand>,

    /// Keys which scroll the attached shell instead of being sent to it
    /// (`keymap.toml`'s `shell_scroll`). Defaults: `M-u` / `M-e`.
    shell_scroll: Vec<ScrollBinding>,

    /// Keys of the shell's scroll mode (`keymap.toml`'s `shell_scroll_mode`),
    /// checked before `shell_scroll` while the mode is active.
    shell_scroll_mode: Vec<ScrollBinding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            navi_maps: HashMap::new(),
            edit_maps: HashMap::new(),
            normal_maps: HashMap::new(),
            shell_scroll: default_scroll_keys(),
            shell_scroll_mode: default_scroll_mode_keys(),
        }
    }
}

impl Keymap {
    pub fn get(&self, key: char) -> anyhow::Result<AppCommand> {
        Ok(
            option_get!(self.normal_maps.get(&key), "Invalid keybinding")
                .to_owned()
        )
    }

    pub fn navi_get(&self, key: char) -> anyhow::Result<AppCommand> {
        Ok(
            option_get!(self.navi_maps.get(&key), "Invalid keybinding")
                .to_owned()
        )
    }

    pub fn edit_get(&self, key: char) -> anyhow::Result<AppCommand> {
        Ok(
            option_get!(self.edit_maps.get(&key), "Invalid keybinding")
                .to_owned()
        )
    }

    /// The scroll command `key` runs while the shell view is live.
    pub fn shell_scroll_get(&self, key: &KeyEvent) -> Option<ScrollCommand> {
        get_binding(&self.shell_scroll, key)
    }

    /// The scroll command `key` runs inside the scroll mode.
    pub fn shell_scroll_mode_get(&self, key: &KeyEvent) -> Option<ScrollCommand> {
        get_binding(&self.shell_scroll_mode, key)
    }

    /// The first key bound to `command` in the scroll mode, for the status
    /// line hint. `None` when the user removed every binding of it.
    pub fn shell_scroll_label(&self, command: ScrollCommand) -> Option<String> {
        self.shell_scroll_mode
            .iter()
            .find(|(_, bound)| *bound == command)
            .map(|(key, _)| key.label())
    }
}

fn get_binding(bindings: &[ScrollBinding], key: &KeyEvent) -> Option<ScrollCommand> {
    bindings
        .iter()
        .find(|(spec, _)| spec.matches(key))
        .map(|(_, command)| *command)
}

fn spec(spec: &str) -> KeySpec {
    KeySpec::parse(spec).expect("the default shell scroll key specs are valid")
}

/// The scroll keys used while the shell is live: the history can be browsed
/// without entering the scroll mode first.
fn default_scroll_keys() -> Vec<ScrollBinding> {
    vec![
        (spec("M-u"), ScrollCommand::Up),
        (spec("M-e"), ScrollCommand::Down),
    ]
}

/// The scroll mode's keys (`prefix + [`).
fn default_scroll_mode_keys() -> Vec<ScrollBinding> {
    vec![
        (spec("k"), ScrollCommand::Up),
        (spec("Up"), ScrollCommand::Up),
        (spec("C-p"), ScrollCommand::Up),
        (spec("j"), ScrollCommand::Down),
        (spec("Down"), ScrollCommand::Down),
        (spec("C-n"), ScrollCommand::Down),
        (spec("PageUp"), ScrollCommand::PageUp),
        (spec("C-b"), ScrollCommand::PageUp),
        (spec("PageDown"), ScrollCommand::PageDown),
        (spec("C-f"), ScrollCommand::PageDown),
        (spec("g"), ScrollCommand::Top),
        (spec("Home"), ScrollCommand::Top),
        (spec("G"), ScrollCommand::Bottom),
        (spec("End"), ScrollCommand::Bottom),
        (spec("q"), ScrollCommand::Quit),
        (spec("Esc"), ScrollCommand::Quit),
        (spec("Enter"), ScrollCommand::Quit),
    ]
}

pub fn init_keymap(app: &mut App, path: String) -> AppResult<()> {
    let err_msg = "The format of content in keymap.toml is error";
    let mut errors = AppError::new();

    let document = get_document(path)?;
    let keymap = document.get("keymap")
        .expect(err_msg)
        .as_array()
        .expect(err_msg);

    for table in keymap.iter() {
        let entry = table.as_inline_table().expect(err_msg);
        let key = entry.get("key")
            .expect(err_msg)
            .as_str()
            .expect(err_msg);
        let bind = entry.get("run")
            .expect(err_msg)
            .as_str()
            .expect(err_msg);

        match AppCommand::from_str(bind) {
            Ok(command) => insert_keybinding(app, key, command, err_msg),
            Err(err) => errors.add_error(err)
        }
    }

    // The shell scroll sections are optional: without them the built-in
    // defaults (M-u/M-e, j/k/PgUp/PgDn/g/G/q…) stay in place, and entries in
    // the file are added on top so a single key can be remapped.
    for (table, mode) in [("shell_scroll", false), ("shell_scroll_mode", true)] {
        let Some(array) = document.get(table) else {
            continue
        };

        let Some(array) = array.as_array() else {
            errors.add_error(anyhow!(
                "`{table}` must be an array of {{ key, run }} tables"
            ));
            continue
        };

        for item in array.iter() {
            let Some(entry) = item.as_inline_table() else {
                errors.add_error(anyhow!(
                    "`{table}` must be an array of {{ key, run }} tables"
                ));
                continue
            };

            let Some(key) = entry.get("key").and_then(|key| key.as_str()) else {
                errors.add_error(anyhow!("`{table}` entry has no `key`"));
                continue
            };

            let Some(run) = entry.get("run").and_then(|run| run.as_str()) else {
                errors.add_error(anyhow!("`{table}` entry has no `run`"));
                continue
            };

            let spec = match KeySpec::parse(key) {
                Ok(spec) => spec,
                Err(err) => {
                    errors.add_error(anyhow!("`{table}`: {err}"));
                    continue
                },
            };

            let command = match ScrollCommand::from_str(run) {
                Ok(command) => command,
                Err(err) => {
                    errors.add_error(anyhow!("`{table}`: {err}"));
                    continue
                },
            };

            let bindings = if mode {
                &mut app.keymap.shell_scroll_mode
            } else {
                &mut app.keymap.shell_scroll
            };

            // A user entry for the same key replaces the default binding
            // instead of adding a second one.
            match bindings.iter_mut().find(|(bound, _)| *bound == spec) {
                Some(slot) => slot.1 = command,
                None => bindings.push((spec, command)),
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

fn insert_keybinding(
    app: &mut App,
    key: &str,
    command: AppCommand,
    err_msg: &str
)
{
    let key_char = key.chars().next().expect(err_msg);
    
    match command {
        AppCommand::NaviIndexInput(_) => {
            app.keymap.navi_maps.insert(key_char, command);
        },

        AppCommand::ShowNaviIndex | AppCommand::MarkExpand => {
            app.keymap.edit_maps.insert(key_char, command.to_owned());
            app.keymap.normal_maps.insert(key_char, command);
        },

        AppCommand::EditMoveItem(_) | AppCommand::EditGotoTop |
        AppCommand::EditGotoBottom | AppCommand::EditMark(_) |
        AppCommand::EditDelete | AppCommand::EditInsert(_) |
        AppCommand::EditNew(_) | AppCommand::EditListScroll(_) |
        AppCommand::QuitEdit =>
        {
            app.keymap.edit_maps.insert(key_char, command);
        },

        _ => {
            app.keymap.normal_maps.insert(key_char, command);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn temp_keymap(content: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-keymap-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("keymap.toml");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_default_scroll_bindings() {
        let keymap = Keymap::default();

        assert_eq!(
            keymap.shell_scroll_get(&key(KeyCode::Char('u'), KeyModifiers::ALT)),
            Some(ScrollCommand::Up)
        );
        assert_eq!(
            keymap.shell_scroll_get(&key(KeyCode::Char('e'), KeyModifiers::ALT)),
            Some(ScrollCommand::Down)
        );
        // Without Alt the key belongs to the shell.
        assert_eq!(
            keymap.shell_scroll_get(&key(KeyCode::Char('u'), KeyModifiers::empty())),
            None
        );

        assert_eq!(
            keymap.shell_scroll_mode_get(&key(KeyCode::PageUp, KeyModifiers::empty())),
            Some(ScrollCommand::PageUp)
        );
        assert_eq!(
            keymap.shell_scroll_mode_get(&key(KeyCode::Char('q'), KeyModifiers::empty())),
            Some(ScrollCommand::Quit)
        );
        // `G` is reported with SHIFT by the terminal.
        assert_eq!(
            keymap.shell_scroll_mode_get(&key(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(ScrollCommand::Bottom)
        );

        assert_eq!(
            keymap.shell_scroll_label(ScrollCommand::Quit),
            Some(String::from("q"))
        );
    }

    #[test]
    fn test_user_bindings_extend_the_defaults() {
        let path = temp_keymap(
            r#"
keymap = [
    { key = "j", run = "move down" },
]

shell_scroll = [
    { key = "M-j", run = "scroll down" },
    { key = "M-u", run = "scroll top" },
]

shell_scroll_mode = [
    { key = "x", run = "scroll page down" },
]
"#,
        );

        let mut app = App::default();
        init_keymap(&mut app, path.to_string_lossy().into_owned()).unwrap();

        assert_eq!(
            app.keymap.shell_scroll_get(&key(KeyCode::Char('j'), KeyModifiers::ALT)),
            Some(ScrollCommand::Down)
        );
        // The defaults which were not overridden are still there.
        assert_eq!(
            app.keymap.shell_scroll_get(&key(KeyCode::Char('e'), KeyModifiers::ALT)),
            Some(ScrollCommand::Down)
        );
        // `M-u` was remapped from `scroll up` to `scroll top`.
        assert_eq!(
            app.keymap.shell_scroll_get(&key(KeyCode::Char('u'), KeyModifiers::ALT)),
            Some(ScrollCommand::Top)
        );
        // Mode defaults stay, the file adds one.
        assert_eq!(
            app.keymap.shell_scroll_mode_get(&key(KeyCode::Char('k'), KeyModifiers::empty())),
            Some(ScrollCommand::Up)
        );
        assert_eq!(
            app.keymap.shell_scroll_mode_get(&key(KeyCode::Char('x'), KeyModifiers::empty())),
            Some(ScrollCommand::PageDown)
        );

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_invalid_scroll_command_is_reported() {
        let path = temp_keymap(
            r#"
keymap = [
    { key = "j", run = "move down" },
]

shell_scroll = [
    { key = "M-j", run = "scroll nowhere" },
]
"#,
        );

        let mut app = App::default();

        assert!(
            init_keymap(&mut app, path.to_string_lossy().into_owned()).is_err()
        );

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
