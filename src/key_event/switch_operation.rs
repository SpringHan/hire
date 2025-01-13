// Switch Operation.

use crate::app::{self, App, AppResult};

#[allow(unused)]
#[derive(PartialEq, Eq, Clone)]
pub enum SwitchCaseData {
    None,
    Bool(bool),
    Number(i32),
    Char(char)
    // DString(String)
}

type FuncPointer = fn(&mut App, char, SwitchCaseData) -> AppResult<bool>;

#[derive(PartialEq, Eq, Clone)]
pub struct SwitchCase(FuncPointer, SwitchCaseData);

impl SwitchCase {
    pub fn new(
        app: &mut App,
        func: FuncPointer,
        msg: String,
        data: SwitchCaseData
    )
    {
        if app.command_error {
            app.command_error = false;
        }

        app.expand_init();
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
