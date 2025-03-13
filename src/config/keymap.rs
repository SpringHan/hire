// Keymap

use std::collections::HashMap;

use crate::{app::App, command::AppCommand, error::AppResult};

pub struct Keymap<'a> {
    maps: HashMap<char, AppCommand<'a>>
}

pub fn init_keymap(app: &mut App, path: String) -> AppResult<()> {
    Ok(())
}
