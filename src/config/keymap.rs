// Keymap

use std::collections::HashMap;

use crate::option_get;
use crate::{error::{AppError, AppResult}, command::AppCommand, app::App};

use super::get_document;

#[derive(Default)]
pub struct Keymap {
    navi_maps: HashMap<char, AppCommand>,
    normal_maps: HashMap<char, AppCommand>
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
            Ok(command) => {
                match command {
                    AppCommand::NaviIndexInput(_) => {
                        app.keymap.navi_maps.insert(
                            key.chars().next().expect(err_msg),
                            command
                        );
                    },

                    _ => {
                        app.keymap.normal_maps.insert(
                            key.chars().next().expect(err_msg),
                            command
                        );
                    }
                }
            },
            Err(err) => errors.add_error(err),
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}
