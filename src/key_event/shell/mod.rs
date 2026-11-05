// Shell operations

mod utils;
mod types;
mod input;
mod graphics;
mod emulation;
mod pty;
mod session;
mod view;
mod menu;
mod mouse;
mod scroll;

pub use types::*;
pub use utils::*;
pub use input::*;
pub use graphics::GraphicsSupport;
pub use session::*;
pub use view::*;
pub use menu::*;
pub use mouse::*;
pub use scroll::ScrollCommand;

use crate::{
    config::{Config, ConfigValue},
    utils::CursorPos,
    error::AppResult,
    app::App,
    rt_error
};

/// Call up the command line for editing shell command.
pub fn cmdline_shell(app: &mut App) -> AppResult<()> {
    let shell_type = Config::get_value(&app.config, "default_shell");

    if let ConfigValue::String(shell_type) = shell_type {
        app.selected_block.set_command_line(
            format!(":!{} ", shell_type),
            CursorPos::End
        );

        return Ok(())
    }

    rt_error!("Wrong value type of `default_shell` config")
}
