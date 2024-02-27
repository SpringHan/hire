// Goto Operation.

use crate::app::{App, OptionFor};
use super::Goto;

use std::error::Error;

pub fn goto_operation(app: &mut App,
                  key: char,
                  in_root: bool
) -> Result<(), Box<dyn Error>>
{
    use toml_edit::{Document, value};

    match key {
        'g' => super::cursor_movement::move_cursor(app, Goto::Index(0), in_root)?,
        'h' => app.goto_dir("/home/spring")?,
        '/' => app.goto_dir("/")?,
        'G' => app.goto_dir("/home/spring/Github")?,
        _ => ()
    }

    app.option_key = OptionFor::None;

    Ok(())
}
