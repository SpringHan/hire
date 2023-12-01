// Command functions

use super::filesaver::sort;
use super::{App, Block, CursorPos};

use std::{io, fs};
use std::path::PathBuf;

/// This enum is used for the errors that will not destroy program.
pub enum OperationError {
    PermissionDenied,
    UnvalidCommand,
    FileExists,
    NoSelected,
    None
}

impl OperationError {
    /// Check whether the OperationError is None
    /// If it's None, return true. Otherwise false.
    pub fn check(self, app: &mut App) -> bool {
        match self {
            OperationError::PermissionDenied => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: Permission Denied!"),
                    CursorPos::End
                );
            },
            OperationError::UnvalidCommand => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: Unvalid Command!"),
                    CursorPos::End
                );
            },
            OperationError::FileExists => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: The File already exists!"),
                    CursorPos::End
                );
            },
            OperationError::NoSelected => {
                app.selected_block = Block::CommandLine(
                    String::from("[Error]: No item to be selected and operated!"),
                    CursorPos::End
                )
            },
            OperationError::None => return true
        }
        app.command_error = true;

        false
    }
}

pub fn rename_file(path: PathBuf,
                   app: &mut App,
                   new_name: String
) -> io::Result<OperationError>
{
    let file = app.get_file_saver_mut();
    if let None = file {
        return Ok(OperationError::NoSelected);
    }

    let file = file.unwrap();
    let is_dir = file.is_dir;

    if file.cannot_read || file.read_only() {
        return Ok(OperationError::PermissionDenied)
    }

    if file.name == new_name {
        return Ok(OperationError::FileExists)
    }

    let origin_file = path.join(&file.name);
    let new_file = path.join(&new_name);
    fs::rename(origin_file, &new_file)?;
    file.name = new_name.to_owned();

    // Refresh modified time
    let metadata = fs::metadata(new_file)?;
    file.set_modified(metadata.modified().unwrap());

    // Refresh the display of whole directory
    let (directory, index) = app.get_directory_mut();
    let mut new_files = directory.to_owned();
    sort(&mut new_files);
    let new_index = new_files
        .iter()
        .position(|x| x.name == new_name && x.is_dir == is_dir)
        .unwrap();
    *directory = new_files;
    index.select(Some(new_index));

    Ok(OperationError::None)
}
