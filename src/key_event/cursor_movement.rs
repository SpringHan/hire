// Cursor Movement.

use std::mem::swap;

use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;

use super::simple_operations::output_path;
use crate::{app::{self, App}, error::AppResult, utils::{get_window_height, Direction}};

type Terminal = RTerminal<CrosstermBackend<std::io::Stderr>>;

/// The enum that used to control move of `List` widget in ratatui.
#[derive(PartialEq, Eq)]
pub enum Goto {
    Up,
    Down,
    ScrollUp,
    ScrollDown,
    Index(usize),
}

pub fn directory_movement(
    direction: Direction,
    app: &mut App,
    terminal: &mut Terminal,
    in_root: bool
) -> AppResult<()>
{
    // TODO: Separate the core code of n & i from directory_movement.
    match direction {
        Direction::Left => {
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
            app.refresh_select_item();

            if app.file_content.is_some() {
                app.file_content.reset();
                app.clean_search_idx();
            }
        },
        Direction::Right => {
            let mut current_empty = false;

            if in_root {
                // It seems impossible that the root directory is empty.
                let selected_file = app.get_file_saver().unwrap();
                if !selected_file.is_dir {
                    if app.output_file.is_some() && app.confirm_output {
                        output_path(app, true)?;
                        return Ok(())
                    }

                    super::shell::open_file_in_shell(
                        app,
                        terminal,
                        app.current_path().join(&selected_file.name)
                    )?;

                    return Ok(())
                }

                if selected_file.cannot_read {
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

                // Open selected file
                if !selected_file.is_dir {
                    if app.output_file.is_some() && app.confirm_output {
                        output_path(app, true)?;
                        return Ok(())
                    }

                    super::shell::open_file_in_shell(
                        app,
                        terminal,
                        app.current_path().join(&selected_file.name)
                    )?;
                    return Ok(())
                }
                
                if selected_file.cannot_read {
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
                app.init_child_files()?;
            }
            app.refresh_select_item();
            app.clean_search_idx();
        },
        Direction::Up => {
            move_cursor(app, Goto::Up, in_root)?;
        },
        Direction::Down => {
            move_cursor(app, Goto::Down, in_root)?;
        },
    }

    Ok(())
}

pub fn move_cursor(
    app: &mut App,
    goto: Goto,
    in_root: bool
) -> AppResult<()>
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

            Goto::ScrollUp => {
                let wind_height = get_window_height() as usize;

                if current_idx < wind_height {
                    selected_item.select_first();
                } else {
                    let after_scroll = current_idx - wind_height;
                    selected_item.select(Some(after_scroll));
                    *selected_item.offset_mut() = after_scroll.saturating_sub(wind_height);
                }
            },

            Goto::ScrollDown => {
                let wind_height = get_window_height() as usize;
                let after_scroll = current_idx.saturating_add(wind_height);
                let current_len = if in_root {
                    app.parent_files.len()
                } else {
                    app.current_files.len()
                };

                if after_scroll >= current_len {
                    selected_item.select(Some(current_len - 1));
                    *selected_item.offset_mut() = after_scroll.saturating_sub(wind_height);
                } else {
                    selected_item.select(Some(after_scroll));
                    *selected_item.offset_mut() = after_scroll;
                }
            },

            Goto::Index(idx) => selected_item.select(Some(idx))
        }

        if in_root {
            let current_file = app.get_file_saver().unwrap();

            if current_file.is_dir {
                app.init_current_files()?;
                app.selected_item.current_select(Some(0));
                if app.file_content.is_some() {
                    app.file_content.reset();
                }
            } else {
                app.set_file_content()?;
            }
            return Ok(())
        }
        
        app.init_child_files()?;
        app.refresh_select_item();
    }

    Ok(())
}
