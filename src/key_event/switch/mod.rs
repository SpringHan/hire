// Switch Operation.

mod traits;

use crate::{app::{self, App}, error::AppResult};

pub use traits::SwitchStruct;

type FuncPointer = fn(&mut App, char, SwitchCaseData) -> AppResult<bool>;

// Struct & enum
pub enum SwitchCaseData {
    None,
    Char(char),
    Struct(Box<dyn SwitchStruct>)
    // Bool(bool),
    // Number(i32),
    // DString(String)
}

#[derive(Clone)]
pub struct SwitchCase(FuncPointer, SwitchCaseData);

impl SwitchCase {
    pub fn new(
        app: &mut App,
        func: FuncPointer,
        expand: bool,
        msg: String,
        data: SwitchCaseData
    )
    {
        if app.command_error {
            app.command_error = false;
        }

        if expand {
            app.expand_init();
        } else {
            // When do not expand command line, turn on command_warning to get confirm.
            app.expand_quit();
            app.command_warning = true;
        }

        app.selected_block = app::Block::CommandLine(msg, app::CursorPos::None);
        app.option_key = app::OptionFor::Switch(SwitchCase(func, data));
    }
}

pub fn switch_match(
    app: &mut App,
    case: SwitchCase,
    key: char
) -> AppResult<()>
{
    let SwitchCase(func, data) = case;

    // Avoid missing error message.
    // In the meanwhile, do not quit command mode if the function returned false.
    if !app.command_error && func(app, key, data)? {
        app.quit_command_mode();
    }

    Ok(())
}
