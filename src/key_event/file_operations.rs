// File operations.

use crate::app::{self, App, CursorPos, FileOperation, MarkedFiles, OptionFor};
use crate::app::command::OperationError;
use super::Goto;
use super::cursor_movement;

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use std::error::Error;
use std::collections::HashMap;

pub fn append_file_name(app: &mut App, to_end: bool) {
    let file_saver = app.get_file_saver();
    if let Some(file_saver) = file_saver {
        let current_file = &file_saver.name;

        if file_saver.is_dir || to_end {
            app.set_command_line(
                format!(":rename {}", current_file),
                CursorPos::End
            );
            return ()
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
        OperationError::NoSelected.check(app);
    }
}


pub fn delete_operation(app: &mut App, key: char) -> Result<(), Box<dyn Error>> {
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
                    OperationError::NoSelected.check(app);
                    app.option_key = OptionFor::None;
                    return Ok(())
                }
            }

            app.marked_operation = FileOperation::Move;
            cursor_movement::move_cursor(app, Goto::Down, in_root)?;
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
                app.goto_dir(current_dir)?;
                app.marked_files.clear();

                app.option_key = OptionFor::None;
                return Ok(())
            }

            let current_file = app.get_file_saver();
            if let Some(current_file) = current_file.cloned() {
                if current_file.cannot_read || current_file.read_only() {
                    OperationError::PermissionDenied(None).check(app);
                    app.option_key = OptionFor::None;
                    return Ok(())
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
                OperationError::NoSelected.check(app);
            }
        },
        _ => ()
    }

    app.option_key = OptionFor::None;
    Ok(())
}

/// Execute mark operation.
/// single is a boolean which indicates whether to mark all files in current dir.
pub fn mark_operation(app: &mut App,
                      single: bool,
                      in_root: bool
) -> Result<(), Box<dyn Error>>
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
            cursor_movement::move_cursor(app, Goto::Down, in_root)?;
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

    OperationError::NoSelected.check(app);
    Ok(())
}

pub fn delete_file<I>(app: &mut App,
                      path: PathBuf,
                      file_iter: I,
                      single_file: bool,
                      in_root: bool
) -> io::Result<()>
where I: Iterator<Item = (String, bool)>
{
    use std::fs::{remove_file, remove_dir_all};

    let mut no_permission_files: Vec<String> = Vec::new();
    let mut not_found_files: Vec<String> = Vec::new();

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
                if err.kind() == ErrorKind::PermissionDenied {
                    no_permission_files.push(file.0);
                } else if err.kind() != ErrorKind::NotFound {
                    not_found_files.push(file.0);
                }

                // When the file does not exist, maybe the path is deleted.
                // If it's true, do not return error for stably running.
                if single_file {
                    app.option_key = OptionFor::None;
                    return Ok(())
                }
            },
            Ok(_) => (),
        }
    }

    if !no_permission_files.is_empty() {
        OperationError::PermissionDenied(Some(no_permission_files)).check(app);
    }

    if !not_found_files.is_empty() {
        OperationError::NotFound(Some(not_found_files)).check(app);
    }

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

pub fn paste_operation(app: &mut App, key: char) -> Result<(), Box<dyn Error>> {
    if app.marked_files.is_empty() || app.marked_operation != FileOperation::Move {
        OperationError::NoSelected.check(app);
        app.option_key = OptionFor::None;
        return Ok(())
    }

    let current_dir = app.current_path();
    let files = app.marked_files.to_owned();

    match key {
        'p' => {
            let exists_files = paste_files(
                app,
                files.iter(),
                current_dir,
                false
            )?;

            for (path, files) in files.into_iter() {
                // Avoid removing files that failed to be moved to target path.
                let path_in_exists = exists_files.get(&path);
                let files: HashMap<String, bool> = if let
                    Some(exists) = path_in_exists
                {
                    files.files
                        .into_iter()
                        .filter(|file|
                                !exists.contains(&file.0))
                        .collect()
                } else {
                    files.files
                };

                delete_file(
                    app,
                    path,
                    files.into_iter(),
                    false,
                    false       // Not necesary
                )?;
            }

            let mut files_for_error: Vec<String> = Vec::new();
            for (_, files) in exists_files.into_iter() {
                files_for_error.extend(files);
            }

            if !files_for_error.is_empty() {
                OperationError::FileExists(files_for_error).check(app);
            }
        },
        's' => {
            let mut final_files: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (path, files) in files.into_iter() {
                for (file, _) in files.files.into_iter() {
                    final_files.push((path.join(&file), current_dir.join(file)));
                }
            }

            app::command::create_symlink(app, final_files.into_iter())?.check(app);
        },
        'c' => {
            paste_files(
                app,
                files.iter(),
                current_dir,
                false
            )?;
        },
        'o' => {
            paste_files(
                app,
                files.iter(),
                current_dir,
                true
            )?;
        },
        'O' => {
            paste_files(
                app,
                files.iter(),
                current_dir,
                true
            )?;

            for (path, files) in files.into_iter() {
                delete_file(
                    app,
                    path,
                    files.files.into_iter(),
                    false,
                    false       // Not necesary
                )?;
            }
        },
        _ => ()
    }

    app.marked_files.clear();
    app.option_key = OptionFor::None;
    app.marked_operation = FileOperation::None;
    app.goto_dir(app.current_path())?;
    Ok(())
}

pub fn paste_files<'a, I, P>(app: &'a mut App,
                             file_iter: I,
                             target_path: P,
                             overwrite: bool
) -> io::Result<HashMap<PathBuf, Vec<String>>>
where
    I: Iterator<Item = (&'a PathBuf, &'a MarkedFiles)>,
    P: AsRef<Path>
{
    use copy_dir::copy_dir;

    // TODO: Record the existed files, return them. Make sure they're not deleted.
    let mut permission_err: Vec<String> = Vec::new();
    let mut exists_files: HashMap<PathBuf, Vec<String>> = HashMap::new();

    macro_rules! file_action {
        ($func:expr, $file:expr, $from:expr $(, $to:expr )*) => {
            match $func($from, $( $to )*) {
                Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                    permission_err.push($file.0.to_owned());
                    continue;
                },
                Ok(_) => (),
                Err(err)=> return Err(err)
            }
        }
    }

    for (path, files) in file_iter {
        let mut target_exists = false;
        let mut target_is_dir = false;

        for file in files.files.iter() {
            let target_file = fs::metadata(
                target_path.as_ref().join(file.0)
            );
            // Check whether the target file exists.
            match target_file {
                Err(err) => {
                    match err.kind() {
                        ErrorKind::PermissionDenied => {
                            permission_err.push(file.0.to_owned());
                            continue;
                        },
                        ErrorKind::NotFound => (), // Nice find.
                        _ => panic!("Unknown error!")
                    }
                },
                Ok(metadata) => {
                    if !overwrite {
                        exists_files
                            .entry(path.to_owned())
                            .or_insert(Vec::new())
                            .push(file.0.to_owned());
                        continue;
                    }
                    target_exists = true;
                    target_is_dir = metadata.is_dir();
                }
            }

            if target_exists {
                if target_is_dir {
                    file_action!(
                        fs::remove_dir_all,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                } else {
                    file_action!(
                        fs::remove_file,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                }
            }

            if *file.1 {         // The original file is a dir.
                file_action!(
                    copy_dir,
                    file,
                    path.join(&file.0),
                    target_path.as_ref().join(&file.0)
                );
            } else {
                file_action!(
                    fs::copy,
                    file,
                    path.join(&file.0),
                    target_path.as_ref().join(&file.0)
                );
            }
        }
    }

    if !permission_err.is_empty() {
        OperationError::PermissionDenied(Some(permission_err)).check(app);
    }

    Ok(exists_files)
}

pub fn make_single_symlink(app: &mut App) -> io::Result<()> {
    if app.marked_files.is_empty() {
        OperationError::NoSelected.check(app);
        return Ok(())
    }

    if app.marked_files.len() > 1 {
        OperationError::Specific(
            String::from("The number of marked files is more than one!")
        ).check(app);
        return Ok(())
    }

    for (path, files) in app.marked_files.iter() {
        for (file, _) in files.files.iter() {
            let original_file = path.join(file);
            app.set_command_line(
                format!(
                    ":create_symlink {} -> {}",
                    original_file.to_string_lossy(),
                    app.current_path().join(file).to_string_lossy()
                ),
                CursorPos::End
            );
            return Ok(())
        }
    }

    Ok(())
}
