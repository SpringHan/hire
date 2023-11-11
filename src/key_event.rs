// Key Event

use crate::App;

use std::{ops::{AddAssign, SubAssign}, borrow::BorrowMut};

use crossterm::{
    event::{KeyCode}
};

/// Handle KEY event.
pub fn handle_event(key: KeyCode, app: &mut App) {
    if let KeyCode::Char(c) = key {
        match c {
            'n' | 'i' | 'u' | 'e' => {
                directory_movement(c, app);
            },
            _ => ()
        }
    }
}

fn directory_movement(direction: char, app: &mut App) {
    let selected_item = &mut app.selected_item;

    match direction {
        'n' => {
            
        },
        'i' => {
            
        },
        'u' => {
            if let Some(ref mut current_item) = selected_item.current {
                if *current_item > 0 {
                    current_item.sub_assign(1);
                    let _ = app.init_child_files(None);
                    app.refresh_select_item();
                }
            }
        },
        'e' => {
            if let Some(ref mut current_item) = selected_item.current {
                if *current_item < app.current_files.len() - 1 {
                    current_item.add_assign(1);
                    let _ = app.init_child_files(None);
                    app.refresh_select_item();
                }
            }
        },

        _ => panic!("Unknown error!")
    }
}
