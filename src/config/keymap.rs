// Keymap

use std::collections::HashMap;

use crate::{app::App, command::AppCommand};

use super::get_document;

#[derive(Default)]
pub struct Keymap<'a> {
    maps: HashMap<char, AppCommand<'a>>
}

pub fn init_keymap(app: &mut App, path: String) -> anyhow::Result<()> {
    let err_msg = "The format of content in keymap.toml is error";

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

        app.keymap.maps.insert(
            key.chars().next().expect(err_msg),
            AppCommand::from_str(bind)?
        );
    }

    Ok(())
}
