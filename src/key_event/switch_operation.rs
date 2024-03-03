// Switch Operation.

use crate::app::App;

use std::error::Error;

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwitchCase(fn(&mut App, char) -> Result<(), Box<dyn Error>>);

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
