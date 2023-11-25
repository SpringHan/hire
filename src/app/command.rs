// Command functions

use super::filesaver::FileSaver;
use super::{App, Block, CursorPos};

use std::{io, fs};
use std::path::PathBuf;

/// This enum is used for the errors that will not destroy program.
pub enum ModificationError {
    PermissionDenied,
    UnvalidCommand,
    None
}

impl ModificationError {
    /// Check whether the ModificationError is None
    /// If it's None, return true. Otherwise false.
    pub fn check(self, app: &mut App) -> bool {
        match self {
            ModificationError::PermissionDenied => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: Permission Denied!"),
                    CursorPos::End
                );
            },
            ModificationError::UnvalidCommand => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: Unvalid Command!"),
                    CursorPos::End
                );
            },
            ModificationError::None => return true
        }
        app.command_error = true;

        false
    }
}

pub fn rename_file(path: PathBuf,
                   file: &mut FileSaver,
                   new_name: String
) -> io::Result<ModificationError>
{
    if file.cannot_read || file.read_only() {
        return Ok(ModificationError::PermissionDenied)
    }

    let origin_file = fs::File::open(
        path.join(&file.name)
    )?;

    Ok(ModificationError::None)
}
