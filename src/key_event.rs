// Key Event

use crate::App;
use crate::app::{self, CursorPos, OptionFor};
use crate::app::command::ModificationError;

use std::mem::swap;
use std::error::Error;
use std::path::PathBuf;
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
                    '/' => app.set_command_line("/", CursorPos::End),
                    'k' => app.next_candidate()?,
                    'K' => app.prev_candidate()?,
                    'a' => append_file_name(app, false),
                    'A' => append_file_name(app, true),
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
                _ => ()
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
        ModificationError::NoSelected.check(app);
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

// TODO: Cannot be used.
fn delete_operation(app: &mut App,
                    key: char,
                    in_root: bool
) -> Result<(), Box<dyn Error>>
{
    use std::fs::remove_file;

    match key {
        'd' => {
            
        },
        'D' => {
            let current_file = app.get_file_saver().clone();

            if let Some(current_file) = current_file {
                if current_file.cannot_read || current_file.read_only() {
                    ModificationError::PermissionDenied.check(app);
                    app.option_key = OptionFor::None;
                    return Ok(())
                }

                remove_file(app.path.join(&current_file.name))?;
                let (dir, idx) = app.get_directory_mut();
                dir.remove(idx.selected().unwrap());

                if dir.is_empty() {
                }

                if dir.len() - 1 == idx.selected().unwrap() {
                }
            } else {
                ModificationError::NoSelected.check(app);
            }
        },
        _ => ()
    }

    Ok(())
}
