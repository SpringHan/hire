// File operations.

use crate::app::{
    App,
    CursorPos,
    FileOperation,
    // OptionFor,
    AppResult,
    AppError,
    ErrorType,
    NotFoundType
};

use super::Goto;
use super::cursor_movement;
use super::{SwitchCase, SwitchCaseData};

use std::io::{self, ErrorKind};
use std::path::PathBuf;

use std::error::Error;
use std::collections::HashMap;

// File name modify
pub fn append_file_name(app: &mut App, to_end: bool) -> AppResult<()> {
    let file_saver = app.get_file_saver();
    if let Some(file_saver) = file_saver {
        let current_file = &file_saver.name;

        if file_saver.is_dir || to_end {
            app.set_command_line(
                format!(":rename {}", current_file),
                CursorPos::End
            );
            return Ok(())
        }

        let cursor_pos = current_file
            .chars()
            .rev()
            .position(|x| x == '.');

        app.set_command_line(
            format!(":rename {}", current_file),
            if let Some(idx) = cursor_pos {
                let idx = current_file.len() - 1 - idx;
                // In this condition,
                // the file does not have string about its extension.
                if idx == 0 {
                    CursorPos::End
                } else {
                    CursorPos::Index(idx + 8)
                }
            } else {
                CursorPos::End
            }
        );
    } else {
        // ErrorType::NoSelected.check(app);
        return Err(ErrorType::NoSelected.pack())
    }

    Ok(())
}

// Delete operation
pub fn delete_operation(app: &mut App) {
    SwitchCase::new(app, delete_switch, generate_msg(), SwitchCaseData::None);
}

fn delete_switch(
    app: &mut App,
    key: char,
    _: SwitchCaseData
) -> AppResult<bool>
{
    let in_root = app.path.to_string_lossy() == "/";

    match key {
        'd' => {
            // Check whether the target dir is accessible firstly.
            if app.marked_files.is_empty() {
                let current_file = app.get_file_saver();
                if let Some(current_file) = current_file {
                    app.append_marked_file(
                        current_file.name.to_owned(),
                        current_file.is_dir
                    );
                } else {
                    return Err(ErrorType::NoSelected.pack())
                    // ErrorType::NoSelected.check(app);
                    // return Ok(false)
                }
            }

            app.marked_operation = FileOperation::Move;
            // TODO: Modify here
            // cursor_movement::move_cursor(app, Goto::Down, in_root)?;
        },
        'D' => {
            if !app.marked_files.is_empty() {
                let current_dir = app.current_path();
                let marked_files = app.marked_files.clone();
                for (path, files) in marked_files.into_iter() {
                    delete_file(
                        app,
                        path,
                        files.files.into_iter(),
                        false,
                        in_root
                    )?;
                }
                app.goto_dir(current_dir, None)?;
                app.marked_files.clear();

                return Ok(true)
            }

            let current_file = app.get_file_saver();
            if let Some(current_file) = current_file.cloned() {
                if current_file.cannot_read || current_file.read_only() {
                    return Err(ErrorType::PermissionDenied(None).pack())
                    // ErrorType::PermissionDenied(None).check(app);
                    // return Ok(false)
                }

                let mut temp_hashmap = HashMap::new();
                temp_hashmap.insert(current_file.name, current_file.is_dir);

                delete_file(
                    app,
                    app.current_path(),
                    temp_hashmap.into_iter(),
                    true,
                    in_root
                )?;
            } else {
                return Err(ErrorType::NoSelected.pack())
                // ErrorType::NoSelected.check(app);
                // return Ok(false)
            }
        },
        _ => ()
    }

    Ok(true)
}

fn generate_msg() -> String {
    let msg = String::from("[d] mark files  [D] delete files");

    msg
}


/// Execute mark operation.
/// single is a boolean which indicates whether to mark all files in current dir.
pub fn mark_operation(app: &mut App,
                      single: bool,
                      in_root: bool
) -> AppResult<()>
{
    if single {
        let selected_file = app.get_file_saver();
        if let Some(selected_file) = selected_file {
            if app.marked_file_contains(&selected_file.name) {
                app.remove_marked_file(selected_file.name.to_owned());
            } else {
                app.append_marked_file(
                    selected_file.name.to_owned(),
                    selected_file.is_dir
                );
            }
            // TODO: Modify here
            // cursor_movement::move_cursor(app, Goto::Down, in_root)?;
            return Ok(())
        }
    } else if !app.current_files.is_empty() {
        // NOTE(for refactoring): Maybe append all files to marked files could be implied in app method.
        if app.marked_file_contains_path() {
            app.clear_path_marked_files();
        } else {
            app.append_marked_files(app.current_files.to_owned().into_iter());
        }
        return Ok(())
    }

    Err(ErrorType::NoSelected.pack())
    // ErrorType::NoSelected.check(app);
    // Ok(())
}

pub fn delete_file<I>(app: &mut App,
                      path: PathBuf,
                      file_iter: I,
                      single_file: bool,
                      in_root: bool
) -> AppResult<()>
where I: Iterator<Item = (String, bool)>
{
    use std::fs::{remove_file, remove_dir_all};

    let mut errors = AppError::new();
    // let mut no_permission_files: Vec<String> = Vec::new();
    // let mut not_found_files: Vec<String> = Vec::new();

    for file in file_iter {
        let is_dir = file.1;
        let full_file = path.join(&file.0);

        let remove_result = if is_dir {
            remove_dir_all(full_file)
        } else {
            remove_file(full_file)
        };

        match remove_result {
            Err(err) => {
                // if err.kind() == ErrorKind::PermissionDenied {
                //     no_permission_files.push(file.0);
                // } else if err.kind() != ErrorKind::NotFound {
                //     not_found_files.push(file.0);
                // }
                errors.add_error(err);

                // When the file does not exist, maybe the path is deleted.
                // If it's true, do not return error for stably running.
                // if single_file {
                //     app.option_key = OptionFor::None;
                //     return Ok(())
                // }
            },
            Ok(_) => (),
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }
    // if !no_permission_files.is_empty() {
    //     ErrorType::PermissionDenied(Some(no_permission_files)).check(app);
    // }

    // if !not_found_files.is_empty() {
    //     ErrorType::NotFound(
    //         NotFoundType::Files(not_found_files)
    //     ).check(app);
    // }

    if !single_file {
        return Ok(())
    }

    let (dir, idx) = app.get_directory_mut();
    dir.remove(idx.selected().unwrap());

    if dir.is_empty() {
        app.selected_item.current_select(None);
        app.selected_item.child_select(None);
        // It's impossible that root directory could be empty.
        app.child_files.clear();

        if app.file_content.is_some() {
            app.file_content = None;
        }
    } else if dir.len() == idx.selected().unwrap() { // There have been an element deleted.
        idx.select(Some(idx.selected().unwrap() - 1));
        if in_root {
            let current_select = app.get_file_saver().unwrap();
            if current_select.is_dir {
                app.init_current_files()?;
            } else {
                app.selected_item.current_select(None);
                app.current_files.clear();
            }
        } else {
            app.init_child_files()?;
            app.selected_item.child_select(None);
        }
        app.init_child_files()?;
        app.refresh_select_item();
    } else {
        app.init_child_files()?;
        app.selected_item.child_select(None);
        app.refresh_select_item();
    }

    Ok(())
}
