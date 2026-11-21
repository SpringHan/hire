// Goto Operation.

use std::path::PathBuf;

use toml_edit::{value, DocumentMut};
use ratatui::{style::Stylize, text::Line};

use super::Goto;
use super::switch::{MenuList, MenuPresentation, SwitchCase, SwitchCaseData};

use crate::app::App;
use crate::config::{get_document, write_document};
use crate::error::{AppError, AppResult, ErrorType, NotFoundType};
use crate::ui::key_hints;

pub fn goto_operation(app: &mut App) {
    SwitchCase::new(
        app,
        goto_switch,
        MenuPresentation::Picker,
        "Goto",
        generate_menu(app, '\0'),
        SwitchCaseData::Char('\0')
    );
}

fn goto_switch(app: &mut App, key: char, data: SwitchCaseData) -> AppResult<bool> {
    let modify_type = if let SwitchCaseData::Char(_data) = data {
        _data
    } else {
        panic!("[1] Unexpected error at goto_switch function in goto_operation.rs.")
    };

    match key {
        'g' => super::cursor_movement::move_cursor(
            app,
            Goto::Index(0),
            app.path.to_string_lossy() == "/"
        )?,
        '+' => {
            SwitchCase::new(
                app,
                goto_switch,
                MenuPresentation::Picker,
                "Add Bookmark",
                generate_menu(app, '+'),
                SwitchCaseData::Char('+')
            );

            return Ok(false)
        },
        '-' => {
            SwitchCase::new(
                app,
                goto_switch,
                MenuPresentation::Picker,
                "Remove Bookmark",
                generate_menu(app, '-'),
                SwitchCaseData::Char('-')
            );

            return Ok(false)
        },
        k => {
            match modify_type {
                '+' => {
                    add_target_dir(
                        app,
                        key,
                        if app.path.to_string_lossy() == "/" {
                            PathBuf::from("/")
                        } else {
                            app.current_path()
                        }
                    )?;
                },
                '-' => remove_target_dir(app, key)?,
                '\0' => {
                    let target_path = app.target_dir.get(&k).cloned();
                    if let Some(path) = target_path {
                        app.record_path_history();
                        app.goto_dir(path, None)?;
                    }
                },
                _ => panic!("[2] Unexpected error at goto_switch in goto_operation.rs.")
            }
        }
    }

    Ok(true)
}

/// The bookmark menu: the hint line, the mode notice and one entry per
/// bookmark.
fn generate_menu(app: &App, status: char) -> MenuList {
    let mut menu = MenuList::new()
        .header(key_hints(
            "[g] goto top  [+] add directory  [-] remove directory",
            app.term_colors.marked_style
        ))
        .header(Line::default());

    match status {
        '+' => {
            menu = menu.header(Line::raw("Adding Directory!").red());
            menu = menu.header(Line::default());
        },
        '-' => {
            menu = menu.header(Line::raw("Deleting Directory!").red());
            menu = menu.header(Line::default());
        },

        _ => ()
    }

    for (key, path) in app.target_dir.iter() {
        menu = menu.entry(key.to_string(), Line::raw(path.clone()));
    }

    menu
}

fn add_target_dir(app: &mut App, key: char, path: PathBuf) -> AppResult<()> {
    let path = path.to_str().unwrap().to_owned();

    let mut toml_config = get_document(app.config_path.to_owned())?;
    toml_config["goto_dir"][String::from(key)] = value(&path);
    
    write_document(toml_config)?;
    app.target_dir.entry(key).insert_entry(path);

    Ok(())
}

fn remove_target_dir(app: &mut App, key: char) -> AppResult<()> {
    let mut toml_config = get_document(app.config_path.to_owned())?;
    if let Some(target_dir) = toml_config.get_mut("goto_dir") {
        target_dir[&String::from(key)] = toml_edit::Item::None;
    } else {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    write_document(toml_config)?;
    app.target_dir.remove(&key);

    Ok(())
}

/// Read config for goto operation.
pub fn read_config(app: &mut App, document: &DocumentMut) -> AppResult<()> {
    let mut errors = AppError::new();

    if let Some(item) = document.get("goto_dir") {
        let Some(inline_table) = item.as_inline_table() else {
            return Err(anyhow::anyhow!(
                "Wrong type for goto_dir config"
            ).into())
        };

        for (key, value) in inline_table.iter() {
            // A broken bookmark is skipped: the other ones are still usable.
            let Ok(key) = key.parse::<char>() else {
                errors.add_error(anyhow::anyhow!(
                    "The key `{key}` of goto_dir is not a single character"
                ));
                continue
            };

            let Some(path) = value.as_str() else {
                errors.add_error(anyhow::anyhow!(
                    "The value of goto_dir key `{key}` must be a string"
                ));
                continue
            };

            app.target_dir.entry(key).or_insert(path.to_owned());
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

#[cfg(test)]
mod goto_test {
    use super::*;

    #[test]
    fn test_read_config() {
        // The config is built in memory, so the test never reads (nor depends
        // on) the real user config, which may open a GUI program.
        let document: DocumentMut = r#"
            goto_dir = { h = "/home", G = "/home/user/Github" }
        "#
            .parse()
            .expect("Invalid TOML document");

        let mut app = App::default();
        read_config(&mut app, &document).expect("Failed to read goto_dir");

        assert_eq!(
            app.target_dir.get(&'h').map(String::as_str),
            Some("/home")
        );
        assert_eq!(
            app.target_dir.get(&'G').map(String::as_str),
            Some("/home/user/Github")
        );
    }

    #[test]
    fn test_read_config_skips_broken_bookmarks() {
        let document: DocumentMut = r#"
            goto_dir = { h = "/home", toolong = "/tmp", d = 3 }
        "#
            .parse()
            .expect("Invalid TOML document");

        let mut app = App::default();
        let err = read_config(&mut app, &document).unwrap_err();

        assert_eq!(
            app.target_dir.get(&'h').map(String::as_str),
            Some("/home")
        );
        assert_eq!(app.target_dir.len(), 1);
        assert!(err.to_string().contains("goto_dir"), "{err}");
    }

    #[test]
    fn test_truncate() {
        let a = "\n".to_owned();
        assert_eq!("", a.trim());
    }

    /// The bookmark menu lists every bookmark with the key which activates it.
    #[test]
    fn test_menu_lists_the_bookmarks() {
        let mut app = App::default();
        app.target_dir.insert('h', String::from("/home"));
        app.target_dir.insert('G', String::from("/home/user/Github"));

        let menu = generate_menu(&app, '\0');

        let keys = menu.rows().iter().filter_map(|row| match row {
            crate::key_event::MenuRow::Entry(entry) => Some(entry.key.as_str()),
            crate::key_event::MenuRow::Line(_) => None,
        }).collect::<Vec<_>>();

        assert_eq!(menu.entry_count(), 2);
        assert!(keys.contains(&"h"), "{keys:?}");
        assert!(keys.contains(&"G"), "{keys:?}");
    }
}
