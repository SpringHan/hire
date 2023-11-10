// Key Event

use crate::App;

use crossterm::{
    event::{KeyCode}
};

/// Handle KEY event.
pub fn handle_event(key: KeyCode, app: &mut App) {
}

fn directory_movement(direction: char, app: &mut App) {
    let selected_item = &mut app.selected_item;

    match direction {
        'n' => {
            
        },
        'i' => {
            
        },
        'u' => {
            
        },
        'e' => {
            
        },

        _ => panic!("Unknown error!")
    }
}
