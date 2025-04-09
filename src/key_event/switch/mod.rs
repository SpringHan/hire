// Switch Operation.

mod traits;

use crate::{app::App, error::AppResult, utils::{Block, CmdContent, CursorPos}};

pub use traits::SwitchStruct;

type FuncPointer = fn(&mut App, char, SwitchCaseData) -> AppResult<bool>;

// Struct & enum
pub enum SwitchCaseData {
    None,
    Char(char),
    Bool(bool),
    Struct(Box<dyn SwitchStruct>)
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
        msg: CmdContent,
        data: SwitchCaseData
    )
    {
        if app.command_error {
            app.command_error = false;
        }

        if expand {
            app.expand_init();
        } else {
            app.expand_quit();
        }

        app.selected_block = Block::CommandLine(msg, CursorPos::None);
        app.switch_case = Some(SwitchCase(func, data));
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
