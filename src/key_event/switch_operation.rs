// Switch Operation.

use crate::app::{self, App};

use std::error::Error;

type FuncPointer = fn(&mut App, char) -> Result<(), Box<dyn Error>>;

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwitchCase(FuncPointer);

impl SwitchCase {
    fn new(app: &mut App, func: FuncPointer, msg: String) {
        app.selected_block = app::Block::CommandLine(msg, app::CursorPos::None);
        app.option_key = app::OptionFor::Switch(SwitchCase(func));
    }
}

pub fn switch_match(
    app: &mut App,
    case: SwitchCase,
    key: char
) -> Result<(), Box<dyn Error>>
{
    let SwitchCase(func) = case;
    func(app, key)?;

    Ok(())
}
