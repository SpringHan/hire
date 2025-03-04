// Shell operations

mod utils;

pub use utils::*;

use crate::{
    config::{Config, ConfigValue},
    app::{App, CursorPos},
    error::AppResult,
    rt_error
};

/// Call up the command line for editing shell command.
pub fn cmdline_shell(app: &mut App) -> AppResult<()> {
    let shell_type = Config::get_value(&app.config, "default_shell");

    if let ConfigValue::String(shell_type) = shell_type {
        app.set_command_line(
            format!(":!{} ", shell_type),
            CursorPos::End
        );

        return Ok(())
    }

    rt_error!("Wrong value type of `default_shell` config")
}
