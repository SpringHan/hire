// Key Event

use crate::App;
use crate::app;

use std::ops::{AddAssign, SubAssign};
use std::mem::swap;
use std::error::Error;
use std::path::PathBuf;

use crossterm::{
    event::{KeyCode}
};

/// Handle KEY event.
pub fn handle_event(key: KeyCode, app: &mut App) -> Result<(), Box<dyn Error>> {
    if let KeyCode::Char(c) = key {
        // TODO: Add check for current selected block -> Browser
        match c {
            'n' | 'i' | 'u' | 'e' => {
                directory_movement(c, app)?;
            },
            _ => ()
        }
    }

    Ok(())
}

fn directory_movement(direction: char,
                      app: &mut App
) -> Result<(), Box<dyn Error>>
{
    let selected_item = &mut app.selected_item;

    match direction {
        'n' => {
            if let app::Block::Browser(true) = app.selected_block {
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

            selected_item.child = selected_item.current;
            selected_item.current = selected_item.parent;
            selected_item.parent = None;
            app.init_parent_files()?;
            app.refresh_select_item(true);
        },
        'i' => {
            
        },
        'u' => {
            move_up_and_down(app, true)?;
        },
        'e' => {
            move_up_and_down(app, false)?;
        },

        _ => panic!("Unknown error!")
    }

    Ok(())
}

fn move_up_and_down(app: &mut App, up: bool) -> Result<(), Box<dyn Error>> {
    // Definition of variables used for telling whether the user is in root.
    let in_root = if let app::Block::Browser(true) = app.selected_block {
        true
    } else {
        false
    };

    let selected_item = if in_root {
        &mut app.selected_item.parent
    } else {
        &mut app.selected_item.current
    };

    // CURRENT_ITEM is used for change itself. Cannot used to search.
    if let Some(ref mut current_item) = selected_item {
        if up {
            if *current_item > 0 {
                current_item.sub_assign(1);
            }
        } else {
            let current_len = if in_root {
                app.parent_files.len()
            } else {
                app.current_files.len()
            };

            if *current_item < current_len - 1 {
                current_item.add_assign(1);
            }
        }

        if in_root {
            let current_file = app.parent_files.get(
                app.selected_item.parent.unwrap()
            ).unwrap();

            let extra_path = PathBuf::from(&current_file.name);
            if current_file.is_dir {
                app.init_current_files(Some(extra_path))?;
                app.selected_item.current = Some(0);
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
