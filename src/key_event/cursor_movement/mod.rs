// Cursor Movement.

mod types;

use std::mem::swap;
use std::ops::Range;

use ratatui::backend::CrosstermBackend;
use ratatui::{widgets::ListState, Terminal as RTerminal};

use super::simple_operations::output_path;
use crate::{
    utils::{get_window_height, Direction},
    app::{self, App, MarkedFiles},
    error::AppResult,
    option_get,
    rt_error,
};

pub use types::*;

type Terminal = RTerminal<CrosstermBackend<std::io::Stderr>>;

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

    if selected_item.selected().is_none() {
        return Ok(())
    }

    // Move cursor
    let expand_range = move_cursor_core(
        goto,
        selected_item,
        if in_root {
            app.parent_files.len()
        } else {
            app.current_files.len()
        },
        app.mark_expand
    );

    if let Some(range) = expand_range {
        let entry = app.marked_files.entry(app.path.to_owned())
            .or_insert(MarkedFiles::default());
        let current_files = if in_root {
            &app.parent_files
        } else {
            &app.current_files
        };

        for i in range {
            let file = &current_files[i];
            entry.files.insert(file.name.to_owned(), file.is_dir);
        }
    }

    // Update child block
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

    Ok(())
}

/// Core logic for moving cursor.
/// This function will return a range to mark files if it's needed.
pub fn move_cursor_core(
    direction: Goto,
    selected_item: &mut ListState,
    item_length: usize,
    mark_expand: bool,
) -> Option<Range<usize>> {
    let origin_index = selected_item.selected().unwrap();
    let mut expand_range: Option<Range<usize>> = None;

    match direction {
        Goto::Up => {
            if origin_index > 0 {
                selected_item.select(Some(origin_index - 1));

                if mark_expand {
                    expand_range = Some(Range {
                        start: origin_index - 1,
                        end: origin_index + 1
                    });
                }
            }
        },

        Goto::Down => {
            if origin_index < item_length - 1 {
                selected_item.select(Some(origin_index + 1));

                if mark_expand {
                    expand_range = Some(Range {
                        start: origin_index,
                        end: origin_index + 2
                    });
                }
            }
        },

        Goto::ScrollUp => {
            let wind_height = get_window_height() as usize;
            let after_scroll: usize;

            if origin_index < wind_height {
                after_scroll = 0;
                selected_item.select_first();

            } else {
                after_scroll = origin_index - wind_height;
                selected_item.select(Some(after_scroll));
                *selected_item.offset_mut() = after_scroll.saturating_sub(wind_height);
            }

            if mark_expand && after_scroll != origin_index {
                expand_range = Some(Range {
                    start: 0,
                    end: origin_index + 1
                });
            }
        },

        Goto::ScrollDown => {
            let wind_height = get_window_height() as usize;
            let after_scroll = origin_index.saturating_add(wind_height);

            if after_scroll >= item_length {
                selected_item.select(Some(item_length - 1));
                *selected_item.offset_mut() = item_length.saturating_sub(wind_height);
            } else {
                selected_item.select(Some(after_scroll));
                *selected_item.offset_mut() = after_scroll;
            }

            if mark_expand && after_scroll != origin_index {
                expand_range = Some(Range {
                    start: origin_index,
                    end: after_scroll + 1
                });
            }
        },

        Goto::Index(idx) => {
            selected_item.select(Some(idx));

            if mark_expand && idx != origin_index {
                let (start, end): (usize, usize);
                if idx > origin_index {
                    start = origin_index;
                    end = idx + 1;
                } else {
                    start = idx;
                    end = origin_index + 1;
                }

                expand_range = Some(Range { start, end });
            }
        }
    }

    expand_range
}

/// Jump to specific item with navigation index entered by user.
pub fn jump_to_index(app: &mut App) -> AppResult<bool> {
    let current_state = if app.root() {
        &app.selected_item.parent
    } else {
        &app.selected_item.current
    };

    let current_len = if app.edit_mode.enabled {
        app.edit_mode.len()
    } else {
        if app.root() {
            app.parent_files.len()
        } else {
            app.current_files.len()
        }   
    };

    if current_state.selected().is_none() {
        app.navi_index.reset();
        rt_error!("Cannot get current selected index")
    }

    if current_len == 0 {
        app.navi_index.reset();
        rt_error!("There's no item can be selected")
    }

    let index = option_get!(
        app.navi_index.index(),
        "You have not input the index"
    );
    let mut after_move = current_state.offset() + index;

    if after_move >= current_len {
        after_move = current_len - 1;
    }

    if app.edit_mode.enabled {
        super::edit::item_navigation(app, Goto::Index(after_move))?;
    } else {
        move_cursor(app, Goto::Index(after_move), app.root())?;
    }

    app.navi_index.reset();

    Ok(true)
}
