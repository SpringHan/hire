// Command functions

use super::filesaver::{sort, FileSaver};
use super::{App, Block, CursorPos};

use std::io::ErrorKind;
use std::{io, fs};
use std::path::PathBuf;

/// This enum is used for the errors that will not destroy program.
pub enum OperationError {
    PermissionDenied(Option<Vec<String>>),
    UnvalidCommand,
    FileExists(Vec<Box<str>>),
    NoSelected,
    NotFound(Option<Vec<String>>),
    None
}

impl OperationError {
    pub fn error_value(&self) -> Option<String> {
        match self {
            OperationError::NoSelected => {
                Some(String::from("[Error]: No item to be selected and operated!"),)
            },
            OperationError::FileExists(files) => {
                Some(format!("[Error]: File {:?} already exists!", files))
            },
            OperationError::UnvalidCommand => {
                Some(String::from("[Error]: Unvalid Command!"))
            },
            OperationError::PermissionDenied(files) => {
                if let Some(files) = files {
                    Some(format!("[Error]: Permission Denied: {:?}", files))
                } else {
                    Some(String::from("[Error]: Permission Denied!"))
                }                
            },
            OperationError::NotFound(files) => {
                if let Some(files) = files {
                    Some(format!("[Error]: Not found: {:?}", files))
                } else {
                    Some(String::from("[Error]: Not found file!"))
                }
            },
            OperationError::None => None
        }
    }

    /// Check whether the OperationError is None
    /// If it's None, return true. Otherwise false.
    pub fn check(self, app: &mut App) -> bool {
        if let Some(msg) = self.error_value() {
            if let
                Block::CommandLine(
                    ref mut error,
                    ref mut cursor
                ) = app.selected_block
            {
                if app.command_error {
                    error.push_str(&format!("\n{}", msg));
                } else {
                    *error = msg;
                    *cursor = CursorPos::End;
                }
            }
        } else {
            return true
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
        return Ok(OperationError::PermissionDenied(None))
    }

    if file.name == new_name {
        return Ok(OperationError::FileExists(vec![new_name.into_boxed_str()]))
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

pub fn create_file<'a, I>(app: &mut App,
                          files: I,
                          is_dir: bool
) -> io::Result<OperationError>
where I: Iterator<Item = &'a str>
{
    let path = app.current_path();
    let mut exists_files: Vec<Box<str>> = Vec::new();
    let mut new_files: Vec<FileSaver> = Vec::new();

    for file in files {
        let file = file.trim_start();
        if is_dir {
            match fs::create_dir(path.join(file)) {
                Err(err) => {
                    if err.kind() == ErrorKind::PermissionDenied {
                        return Ok(OperationError::PermissionDenied(None))
                    } else if err.kind() == ErrorKind::AlreadyExists {
                        exists_files.push(file.into());
                    }
                },
                Ok(_) => new_files.push(FileSaver::new(
                    file,
                    path.join(&file),
                    None
                ))
            }
        } else {
            let file_create = fs::File::create(
                path.join(file)
            );
            match file_create {
                Ok(file_create) => {
                    new_files.push(FileSaver::new(
                        file,
                        path.join(&file),
                        Some(file_create.metadata())
                    ));
                },
                Err(err) => {
                    if err.kind() == ErrorKind::PermissionDenied {
                        return Ok(OperationError::PermissionDenied(None))
                    } else if err.kind() == ErrorKind::AlreadyExists {
                        exists_files.push(file.into());
                    }
                }
            }
        }
    }

    // Update render
    let mut prev_file_name = String::from("");
    let mut prev_is_dir = false;
    let current_file = app.get_file_saver();
    if let Some(current_file) = current_file {
        prev_file_name = current_file.name.to_owned();
        prev_is_dir = current_file.is_dir;
    }

    let (dir, idx) = app.get_directory_mut();
    dir.extend(new_files.into_iter());
    sort(dir);
    if prev_file_name.is_empty() {
        idx.select(Some(0));
        if app.path.to_string_lossy() == "/" {
            // NOTE: The first item in root directory must be a dir.
            app.init_current_files(Some(app.parent_files[0].name.to_owned()))?;
            app.selected_item.current_select(Some(0));
        } else {
            app.init_child_files(None)?;
            app.refresh_select_item(false);
        }
    } else {
        idx.select(Some(
            dir.iter()
                .position(|file|
                          file.name == prev_file_name
                          && file.is_dir == prev_is_dir)
                .unwrap()
        ));
        if app.path.to_string_lossy() == "/" {
            app.init_current_files(Some(prev_file_name))?;
            app.selected_item.current_select(Some(0));
        } else {
            app.init_child_files(None)?;
            app.refresh_select_item(false);
        }
    }

    if !exists_files.is_empty() {
        return Ok(OperationError::FileExists(exists_files))
    }

    Ok(OperationError::None)
}
