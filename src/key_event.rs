// Key Event

use crate::App;
use crate::app::{self, CursorPos, OptionFor, FileOperation, MarkedFiles};
use crate::app::command::OperationError;

use std::fs;
use std::mem::swap;
use std::error::Error;
use std::collections::HashMap;
use std::path::{PathBuf, Path};
use std::io::{self, ErrorKind};
use std::ops::{SubAssign, AddAssign};

use crossterm::event::KeyCode;

/// The enum that used to declare method to move.
#[derive(PartialEq, Eq)]
pub enum Goto {
    Up,
    Down,
    Index(usize)
}

// NOTE: When quiting command-line mode, you're required to use quit_command_mode function!
/// Handle KEY event.
pub fn handle_event(key: KeyCode, app: &mut App) -> Result<(), Box<dyn Error>> {
    match key {
        KeyCode::Char(c) => {
            if let app::Block::Browser(in_root) = app.selected_block {
                // NOTE: All the function in the blocks below must be end with
                // code to set OPTION_KEY to None.
                match app.option_key {
                    OptionFor::Goto => {
                        goto_operation(app, c, in_root)?;
                        return Ok(())
                    },
                    OptionFor::Delete => {
                        delete_operation(app, c, in_root)?;
                        return Ok(())
                    },
                    OptionFor::None => ()
                }

                match c {
                    'n' | 'i' | 'u' | 'e' => directory_movement(
                        c, app, in_root
                    )?,
                    'g' => app.option_key = OptionFor::Goto,
                    'G' => {
                        let last_idx = if in_root {
                            app.parent_files.len() - 1
                        } else {
                            app.current_files.len() - 1
                        };
                        move_cursor(app, Goto::Index(last_idx), in_root)?;
                    },
                    'd' => app.option_key = OptionFor::Delete,
                    '/' => app.set_command_line("/", CursorPos::End),
                    'k' => app.next_candidate()?,
                    'K' => app.prev_candidate()?,
                    'a' => append_file_name(app, false),
                    'A' => append_file_name(app, true),
                    ' ' => mark_operation(app, true, in_root)?,
                    'm' => mark_operation(app, false, in_root)?,
                    '+' => app.set_command_line(
                        ":create_dir ",
                        CursorPos::End
                    ),
                    '=' => app.set_command_line(
                        ":create_file ",
                        CursorPos::End
                    ),
                    _ => ()
                }
            } else {
                app.command_line_append(c);
            }
        },

        KeyCode::Backspace => {
            if let
                app::Block::CommandLine(
                    ref mut origin,
                    ref mut cursor
                ) = app.selected_block
            {
                if let app::CursorPos::Index(idx) = cursor {
                    if *idx == 0 {
                        return Ok(())
                    }
                    origin.remove(*idx - 1);
                    idx.sub_assign(1);
                } else {
                    origin.pop();
                }
            }
        },

        KeyCode::Esc => {
            match app.selected_block {
                app::Block::CommandLine(_, _) => {
                    app.quit_command_mode();
                },
                _ => {
                    if !app.marked_files.is_empty() {
                        app.marked_files.clear();
                        app.marked_operation = FileOperation::None;
                    }
                }
            }
        },

        KeyCode::Enter => app.command_parse()?,

        KeyCode::Up => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                app.command_select(Goto::Up);
            }
        },

        KeyCode::Down => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                app.command_select(Goto::Down);
            }
        },

        KeyCode::Left => {
            if let
                app::Block::CommandLine(
                    ref command,
                    ref mut cursor
                ) = app.selected_block
            {
                if let CursorPos::Index(idx) = cursor {
                    if *idx == 0 {
                        return Ok(())
                    }
                    idx.sub_assign(1);
                } else {
                    *cursor = CursorPos::Index(command.len() - 1);
                }
            }
        },

        KeyCode::Right => {
            if let
                app::Block::CommandLine(
                    ref command,
                    ref mut cursor
                ) = app.selected_block
            {
                if let CursorPos::Index(idx) = cursor {
                    if *idx == command.len() - 1 {
                        *cursor = CursorPos::End;
                        return Ok(())
                    }
                    idx.add_assign(1);
                }
            }
        },

        _ => ()
    }

    Ok(())
}

fn directory_movement(direction: char,
                      app: &mut App,
                      in_root: bool
) -> Result<(), Box<dyn Error>>
{
    match direction {
        'n' => {
            if in_root {
                return Ok(())
            }

            let parent_dir = app.path.parent().unwrap().to_path_buf();
            app.path = parent_dir;

            if app.path.to_str() == Some("/") {
                app.selected_block = app::Block::Browser(true);
                return Ok(())
            }

            // TODO: Maybe there could be a better way.
            swap(&mut app.child_files, &mut app.current_files);
            swap(&mut app.current_files, &mut app.parent_files);

            let selected_item = &mut app.selected_item;

            selected_item.child_select(selected_item.current_selected());
            selected_item.current_select(selected_item.parent_selected());
            selected_item.parent_select(None);
            app.init_parent_files()?;
            // Normally, calling this function would initialize child_index.
            // So, use TRUE to keep it.
            app.refresh_select_item(true);

            if app.file_content.is_some() {
                app.file_content = None;
                app.clean_search_idx();
            }
        },
        'i' => {
            let mut current_empty = false;

            if in_root {
                let selected_file = app.get_file_saver().unwrap();
                // It seems impossible that the root directory is empty.
                // if let None = selected_file {
                //     return Ok(())
                // }

                // let selected_file = selected_file.unwrap();
                if !selected_file.is_dir || selected_file.cannot_read {
                    return Ok(())
                }

                app.path = app.path.join(&selected_file.name);
                app.selected_block = app::Block::Browser(false);
            } else {
                let selected_file = app.get_file_saver();
                if let None = selected_file {
                    return Ok(())
                }

                let selected_file = selected_file.unwrap();
                if !selected_file.is_dir || selected_file.cannot_read {
                    return Ok(())
                }

                app.path = app.path.join(selected_file.name.to_owned());
                app.parent_files = Vec::new();
                swap(&mut app.parent_files, &mut app.current_files);
                swap(&mut app.current_files, &mut app.child_files);

                let selected_item = &mut app.selected_item;
                swap(&mut selected_item.parent, &mut selected_item.current);
                selected_item.current_select(selected_item.child_selected());
                if app.current_files.is_empty() {
                    current_empty = true;
                }
            }
            if !current_empty {
                app.init_child_files(None)?;
            }
            app.refresh_select_item(false);
            app.clean_search_idx();
        },
        'u' => {
            move_cursor(app, Goto::Up, in_root)?;
        },
        'e' => {
            move_cursor(app, Goto::Down, in_root)?;
        },

        _ => panic!("Unknown error!")
    }

    Ok(())
}

pub fn move_cursor(app: &mut App,
                   goto: Goto,
                   in_root: bool
) -> Result<(), Box<dyn Error>>
{
    let selected_item = if in_root {
        &mut app.selected_item.parent
    } else {
        if app.current_files.is_empty() {
            return Ok(())
        }

        &mut app.selected_item.current
    };

    // CURRENT_ITEM is used for change itself. Cannot used to search.
    if let Some(current_idx) = selected_item.selected() {
        match goto {
            Goto::Up => {
                if current_idx > 0 {
                    selected_item.select(Some(current_idx - 1));
                }
            },
            Goto::Down => {
                let current_len = if in_root {
                    app.parent_files.len()
                } else {
                    app.current_files.len()
                };

                if current_idx < current_len - 1 {
                    selected_item.select(Some(current_idx + 1));
                }
            },
            Goto::Index(idx) => selected_item.select(Some(idx))
        }

        if in_root {
            let current_file = app.parent_files.get(
                app.selected_item.parent_selected().unwrap()
            ).unwrap();

            let extra_path = PathBuf::from(&current_file.name);
            if current_file.is_dir {
                app.init_current_files(Some(extra_path))?;
                app.selected_item.current_select(Some(0));
                if app.file_content.is_some() {
                    app.file_content = None;
                }
            } else {
                app.set_file_content()?;
            }
            return Ok(())
        }
        
        app.init_child_files(None)?;
        app.refresh_select_item(false);
    }

    Ok(())
}

fn append_file_name(app: &mut App, to_end: bool) {
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

fn goto_operation(app: &mut App,
                  key: char,
                  in_root: bool
) -> Result<(), Box<dyn Error>>
{
    match key {
        'g' => move_cursor(app, Goto::Index(0), in_root)?,
        'h' => app.goto_dir("/home/spring")?,
        '/' => app.goto_dir("/")?,
        'G' => app.goto_dir("/home/spring/Github")?,
        _ => ()
    }

    app.option_key = OptionFor::None;

    Ok(())
}

fn delete_operation(app: &mut App,
                    key: char,
                    in_root: bool
) -> Result<(), Box<dyn Error>>
{
    match key {
        'd' => {
            // NOTE: Check whether the target dir is accessible firstly.
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
            move_cursor(app, Goto::Down, in_root)?;
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
            move_cursor(app, Goto::Down, in_root)?;
            return Ok(())
        }
    } else if !app.current_files.is_empty() {
        // NOTE: Maybe append all files to marked files could be implied in app method.
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

fn delete_file<I>(app: &mut App,
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
                app.init_current_files(Some(current_select.name.clone()))?;
            } else {
                app.selected_item.current_select(None);
                app.current_files.clear();
            }
        } else {
            app.init_child_files(None)?;
            app.selected_item.child_select(None);
        }
        app.init_child_files(None)?;
        app.refresh_select_item(false);
    } else {
        app.init_child_files(None)?;
        app.selected_item.child_select(None);
        app.refresh_select_item(false);
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
            paste_files(
                app,
                files.into_iter(),
                current_dir,
                false
            )?;
            // TODO: Delete files
        },
        's' => {
        },
        'c' => {
            paste_files(
                app,
                files.into_iter(),
                current_dir,
                false
            )?;
        },
        'o' => {
            paste_files(
                app,
                files.into_iter(),
                current_dir,
                true
            )?;
        },
        'O' => {
            paste_files(
                app,
                files.into_iter(),
                current_dir,
                true
            )?;
            // TODO: Delete files
        },
        _ => ()
    }

    app.marked_files.clear();
    app.option_key = OptionFor::None;
    Ok(())
}

fn paste_files<I, P>(app: &mut App,
                     file_iter: I,
                     target_path: P,
                     overwrite: bool
) -> io::Result<()>
where
    I: Iterator<Item = (PathBuf, MarkedFiles)>,
    P: AsRef<Path>
{
    use copy_dir::copy_dir;

    let mut permission_err: Vec<String> = Vec::new();
    let mut exists_files: Vec<Box<str>> = Vec::new();

    macro_rules! file_action {
        ($func:expr, $file:expr, $from:expr $(, $to:expr )*) => {
            match $func($from, $( $to )*) {
                Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                    permission_err.push($file.0);
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

        for file in files.files.into_iter() {
            let target_file = fs::metadata(
                target_path.as_ref().join(&file.0)
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
                        exists_files.push(file.0.into_boxed_str());
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

            if file.1 {         // The original file is a dir.
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

    if !exists_files.is_empty() {
        OperationError::FileExists(exists_files).check(app);
    }

    Ok(())
}
