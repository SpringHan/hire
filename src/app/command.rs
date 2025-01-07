// Command functions

use super::filesaver::{sort, FileSaver};
use super::{App, Block, CursorPos};

use std::io::ErrorKind;
use std::{io, fs};
use std::path::{PathBuf, Path};

/// The three cases matching not found error.
pub enum NotFoundType {
    Files(Vec<String>),
    Item(String),
    None
}

/// This enum is used for the errors that will not destroy program.
pub enum OperationError {
    PermissionDenied(Option<Vec<String>>),
    UnvalidCommand,
    FileExists(Vec<String>),
    NoSelected,
    NotFound(NotFoundType),
    Specific(String),
    None
}

impl OperationError {
    pub fn error_value(&self) -> Option<String> {
        match self {
            OperationError::NoSelected => {
                Some(String::from("[Error]: No item to be selected and operated!"))
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
            OperationError::NotFound(data) => {
                match data {
                    NotFoundType::Files(files) => Some(format!("[Error]: Not found: {:?}", files)),
                    NotFoundType::Item(item) => Some(format!("[Error]: Not found: {}", item)),
                    NotFoundType::None => Some(String::from("[Error]: The file/item cannot be found!")),
                }
            },
            OperationError::Specific(err) => {
                Some(format!("[Error]: {}", err))
            }
            OperationError::None => None
        }
    }

    /// Check whether the OperationError is None
    /// If it's None, return true. If previous errors exist, still return false.
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
                    *cursor = CursorPos::None;
                    app.command_error = true;
                }

                // Turn off Switch mode.
                if let super::OptionFor::Switch(_) = app.option_key {
                    app.option_key = super::OptionFor::None;
                }
            } else {
                app.set_command_line(msg, CursorPos::None);
                app.command_error = true;
            }

            return false
        }

        // Though current error not exists, but previous errors exist.
        if app.command_error {
            return false
        }

        true
    }
}

pub fn rename_file(path: PathBuf,
                   app: &mut App,
                   new_name: String
) -> io::Result<OperationError>
{
    let hide_files = app.hide_files;
    let file = app.get_file_saver_mut();
    if let None = file {
        return Ok(OperationError::NoSelected);
    }

    let file = file.unwrap();

    if file.cannot_read || file.read_only() {
        return Ok(OperationError::PermissionDenied(None))
    }

    if file.name == new_name {
        return Ok(OperationError::FileExists(vec![new_name]))
    }

    let origin_file = path.join(&file.name);
    let new_file = path.join(&new_name);
    fs::rename(origin_file, &new_file)?;
    file.name = new_name.to_owned();

    if new_name.starts_with(".") && hide_files {
        app.hide_or_show(Some(new_name))?;
        return Ok(OperationError::None)
    }

    // Refresh modified time
    let metadata = fs::metadata(new_file)?;
    file.set_modified(metadata.modified().unwrap());

    // Refresh the display of whole directory
    let (directory, index) = app.get_directory_mut();
    let mut new_files = directory.to_owned();
    sort(&mut new_files);
    let new_index = new_files
        .iter()
        .position(|x| x.name == new_name)
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
    let mut exists_files: Vec<String> = Vec::new();
    let mut new_files: Vec<FileSaver> = Vec::new();
    let mut to_show_hidden_files = false;

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

        if !to_show_hidden_files && file.starts_with(".") && app.hide_files {
            to_show_hidden_files = true;
        }
    }

    // Update render
    if !to_show_hidden_files {
        let mut prev_file_name = String::from("");
        let current_file = app.get_file_saver();
        if let Some(current_file) = current_file {
            prev_file_name = current_file.name.to_owned();
        }

        let (dir, idx) = app.get_directory_mut();
        dir.extend(new_files.into_iter());
        sort(dir);
        if prev_file_name.is_empty() {
            idx.select(Some(0));
            if app.path.to_string_lossy() == "/" {
                // NOTE: The first item in root directory must be a dir.
                app.init_current_files()?;
                app.selected_item.current_select(Some(0));
            } else {
                app.init_child_files()?;
                app.refresh_select_item();
            }
        } else {
            idx.select(Some(
                dir.iter()
                    .position(|file|
                              file.name == prev_file_name)
                    .unwrap()
            ));
            if app.path.to_string_lossy() == "/" {
                app.init_current_files()?;
                app.selected_item.current_select(Some(0));
            } else {
                app.init_child_files()?;
                app.refresh_select_item();
            }
        }
    } else {
        app.hide_or_show(None)?;
    }

    if !exists_files.is_empty() {
        return Ok(OperationError::FileExists(exists_files))
    }

    Ok(OperationError::None)
}

pub fn create_symlink<I, P>(app: &mut App, files: I) -> io::Result<OperationError>
where
    I: Iterator<Item = (P, P)>,
    P: AsRef<Path>
{
    use std::os::unix::fs::symlink;

    let mut no_permission: Vec<String> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();
    let mut exists_links: Vec<String> = Vec::new();
    let mut to_show_hidden_files = false;

    for (file, target) in files {
        match symlink(&file, &target) {
            Err(err) => {
                match err.kind() {
                    ErrorKind::PermissionDenied => no_permission.push(
                        file.as_ref().to_string_lossy().into()
                    ),
                    ErrorKind::NotFound => not_found.push(
                        file.as_ref().to_string_lossy().into()
                    ),
                    ErrorKind::AlreadyExists => exists_links.push(
                        file.as_ref().to_string_lossy().into()
                    ),
                    _ => return Err(err)
                }
            },
            _ => {
                if !to_show_hidden_files
                    && target.as_ref().file_name().unwrap().to_string_lossy().starts_with(".")
                    && app.hide_files
                {
                    to_show_hidden_files = true;
                }
            }
        }
    }
    if to_show_hidden_files {
        app.hide_or_show(None)?;
    } else {
        app.partly_update_block()?;
    }

    if !no_permission.is_empty() {
        OperationError::PermissionDenied(Some(no_permission)).check(app);
    }

    if !exists_links.is_empty() {
        OperationError::FileExists(exists_links).check(app);
    }

    if !not_found.is_empty() {
        return Ok(OperationError::NotFound(NotFoundType::Files(not_found)))
    }

    Ok(OperationError::None)
}
