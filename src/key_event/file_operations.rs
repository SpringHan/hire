// File operations.

use std::path::PathBuf;
use std::collections::HashMap;

use ratatui::{style::Stylize, text::{Line, Text}};

use super::Goto;
use super::cursor_movement;
use super::{SwitchCase, SwitchCaseData};

use crate::app::App;
use crate::utils::{CmdContent, CursorPos};
use crate::error::{AppResult, AppError, ErrorType};

// File name modify
pub fn append_file_name(app: &mut App, to_end: bool) -> AppResult<()> {
    let file_saver = app.get_file_saver();
    if let Some(file_saver) = file_saver {
        let current_file = &file_saver.name;

        if file_saver.is_dir || to_end {
            app.selected_block.set_command_line(
                format!(":rename {}", current_file),
                CursorPos::End
            );
            return Ok(())
        }

        let cursor_pos = current_file
            .chars()
            .rev()
            .position(|x| x == '.');

        app.selected_block.set_command_line(
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
        return Err(ErrorType::NoSelected.pack())
    }

    Ok(())
}

// Delete operation
pub fn delete_operation(app: &mut App) {
    SwitchCase::new(
        app,
        delete_switch,
        true,
        generate_msg(app, false),
        SwitchCaseData::None
    );
}

fn delete_switch(
    app: &mut App,
    key: char,
    data: SwitchCaseData
) -> AppResult<bool>
{
    let in_root = app.path.to_string_lossy() == "/";

    match key {
        'D' => {
            if let SwitchCaseData::None = data {
                SwitchCase::new(
                    app,
                    delete_switch,
                    true,
                    generate_msg(app, true),
                    SwitchCaseData::Bool(true)
                );

                return Ok(false)
            }
        },

        'y' => {
            if let SwitchCaseData::Bool(value) = data {
                if !value {
                    return Ok(true)
                }

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
                    app.mark_expand = false;

                    return Ok(true)
                }

                let current_file = app.get_file_saver();
                if let Some(current_file) = current_file.cloned() {
                    if current_file.cannot_read || current_file.read_only() {
                        return Err(ErrorType::PermissionDenied(Vec::new()).pack())
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
                }
            }
        },
        _ => ()
    }

    Ok(true)
}

fn generate_msg(app: &App, confirm: bool) -> CmdContent {
    let mut msg = String::from("[D] delete files\n\n");

    if app.marked_files.is_empty() {
        if let Some(file) = app.get_file_saver() {
            msg.push_str(&format!(
                "{}/{}",
                if app.root() {
                    String::new()
                } else {
                    app.current_path()
                        .to_string_lossy()
                        .to_string()
                },
                file.name
            ));

            if file.is_dir {
                msg.push('/');
            }

            msg.push('\n');
        } else {
            msg.push_str("No file to be deleted!");
        }
    }

    for (path, files) in app.marked_files.iter() {
        for (file, is_dir) in files.files.iter() {
            msg.push_str(&format!(
                "{}/{}",
                if path.to_string_lossy() == "/" {
                    String::new()
                } else {
                    path.to_string_lossy().to_string()
                },
                file
            ));

            if *is_dir {
                msg.push('/');
            }

            msg.push('\n');
        }
    }

    let mut text = Text::raw(msg);

    if confirm {
        text.push_line("");
        text.push_line(Line::raw(
            "Are you sure to remove these files? (y to confirm)"
        ).red());
    }

    CmdContent::Text(text)
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

    Err(ErrorType::NoSelected.pack())
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
            app.file_content.reset();
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
