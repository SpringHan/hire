// Goto Operation.

use std::path::PathBuf;

use anyhow::bail;
use toml_edit::{value, DocumentMut};
use ratatui::{style::Stylize, text::{Line, Text}};

use super::Goto;
use super::{SwitchCase, SwitchCaseData};

use crate::app::{App, CmdContent};
use crate::config::{get_document, write_document};
use crate::error::{AppResult, ErrorType, NotFoundType};

pub fn goto_operation(app: &mut App) {
    SwitchCase::new(
        app,
        goto_switch,
        true,
        generate_msg(app, '\0'),
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
                true,
                generate_msg(app, '+'),
                SwitchCaseData::Char('+')
            );

            return Ok(false)
        },
        '-' => {
            SwitchCase::new(
                app,
                goto_switch,
                true,
                generate_msg(app, '-'),
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
                        app.goto_dir(path, None)?;
                    }
                },
                _ => panic!("[2] Unexpected error at goto_switch in goto_operation.rs.")
            }
        }
    }

    Ok(true)
}

fn generate_msg(app: &App, status: char) -> CmdContent {
    let mut msg = Text::raw("[g] goto top  [+] add directory  [-] remove directory");
    msg.push_line("");

    match status {
        '+' => {
            msg.push_line(Line::raw("Adding Directory!").red());
            msg.push_line("");
        },
        '-' => {
            msg.push_line(Line::raw("Deleting Directory!\n").red());
            msg.push_line("");
        },

        _ => ()
    }

    for (key, path) in app.target_dir.iter() {
        msg.push_line(format!("[{}] {}\n", key, path));
    }

    CmdContent::Text(msg)
}

fn add_target_dir(app: &mut App, key: char, path: PathBuf) -> AppResult<()> {
    let path = path.to_str().unwrap().to_owned();

    let mut toml_config = get_document(app.config_path.to_owned())?;
    toml_config["goto_dir"][String::from(key)] = value(&path);
    
    write_document(toml_config)?;
    app.target_dir.entry(key).or_insert(path);

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
pub fn read_config(app: &mut App, document: &DocumentMut) -> anyhow::Result<()> {
    if let Some(item) = document.get("goto_dir") {
        if let Some(inline_table) = item.as_inline_table() {
            for e in inline_table.into_iter() {
                app.target_dir
                    .entry(e.0.parse().unwrap())
                    .or_insert(
                        e.1
                            .as_str()
                            .expect("Type error for goto_dir config!")
                            .to_owned()
                    );
            }
        } else {
            bail!("Wrong type for goto_dir config")
        }
    }

    Ok(())
}

#[cfg(test)]
mod goto_test {
    use super::*;

    #[test]
    fn test_print() {
        let mut app = App::default();
        crate::config::init_config(&mut app).unwrap();
        // println!("{}\n{:?}", app.config_path, app.target_dir);
        // panic!()
    }

    #[test]
    fn test_truncate() {
        let a = "\n".to_owned();
        assert_eq!("", a.trim());
    }
}
