// Key Event

use crate::App;
use crate::app::{self, CursorPos};

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
                match c {
                    'n' | 'i' | 'u' | 'e' => directory_movement(
                        c, app, in_root
                    )?,
                    'g' => move_cursor(app, Goto::Index(0), in_root)?,
                    'G' => {
                        let last_idx = if in_root {
                            app.parent_files.len() - 1
                        } else {
                            app.current_files.len() - 1
                        };
                        move_cursor(app, Goto::Index(last_idx), in_root)?;
                    },
                    '/' => app.set_command_line("/", CursorPos::End),
                    // '?' => app.set_command_line("?", CursorPos::End),
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

        KeyCode::Enter => {
            if let
                app::Block::CommandLine(ref content, _) = app.selected_block
            {
                if content.starts_with('/') {
                    app.file_search(content[1..].to_owned());
                    // app.next_candidate()?;
                }
                // else if content.starts_with('?') {
                //     app.file_search(content[1..].to_owned());
                //     // app.prev_candidate()?;
                // }
                app.quit_command_mode();
            }
        },

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
    let selected_item = &mut app.selected_item;

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

            selected_item.child_select(selected_item.current_selected());
            selected_item.current_select(selected_item.parent_selected());
            selected_item.parent_select(None);
            app.init_parent_files()?;
            app.refresh_select_item(true);

            if app.file_content.is_some() {
                app.file_content = None;
                app.clean_search_idx();
            }
        },
        'i' => {
            if in_root {
                let selected_file = app.parent_files.get(
                    selected_item.parent_selected().unwrap()
                ).unwrap();
                if !selected_file.is_dir || app.current_files.is_empty() {
                    return Ok(())
                }

                app.path = app.path.join(&selected_file.name);
                app.selected_block = app::Block::Browser(false);
            } else {
                let selected_file = app.current_files.get(
                    selected_item.current_selected().unwrap()
                ).unwrap();
                if !selected_file.is_dir || app.child_files.is_empty() {
                    return Ok(())
                }

                app.path = app.path.join(&selected_file.name);
                app.parent_files = Vec::new();
                swap(&mut app.parent_files, &mut app.current_files);
                swap(&mut app.current_files, &mut app.child_files);
                swap(&mut selected_item.parent, &mut selected_item.current);
                selected_item.current_select(selected_item.child_selected());
            }
            app.init_child_files(None)?;
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

}
