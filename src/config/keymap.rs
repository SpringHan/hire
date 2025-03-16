// Keymap

use std::collections::HashMap;

use crate::{app::App, command::AppCommand, error::{AppError, AppResult}, option_get};

use super::get_document;

#[derive(Default)]
pub struct Keymap {
    maps: HashMap<char, AppCommand>
}

impl Keymap {
    pub fn get(&self, key: char) -> anyhow::Result<AppCommand> {
        Ok(
            option_get!(self.maps.get(&key), "Invalid keybinding")
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
                app.keymap.maps.insert(
                    key.chars().next().expect(err_msg),
                    command
                );
            },
            Err(err) => errors.add_error(err),
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}
