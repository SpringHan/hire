// Switch Operation.

use crate::app::{self, App};

use std::error::Error;

type FuncPointer<T> = fn(&mut App, char, Option<T>) -> Result<bool, Box<dyn Error>>;

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwitchCase<T>(FuncPointer<T>, Option<T>);

impl<T> SwitchCase<T> {
    pub fn new(app: &mut App, func: FuncPointer<T>, msg: String, data: Option<T>) {
        if app.command_error {
            app.command_error = false;
        }

        app.expand_init();
        app.selected_block = app::Block::CommandLine(msg, app::CursorPos::None);
        app.option_key = app::OptionFor::Switch(SwitchCase(func, data));
    }
}

pub fn switch_match<T>(
    app: &mut App,
    case: SwitchCase<T>,
    key: char
) -> Result<(), Box<dyn Error>>
{
    let SwitchCase(func, data) = case;

    // Avoid missing error message.
    // In the meanwhile, do not quit command mode if the function returned false.
    if !app.command_error && func(app, key, data)? {
        app.quit_command_mode();
    }

    Ok(())
}
