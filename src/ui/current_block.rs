// Current Block

use ratatui::{
    symbols::{border::{self, Set}, line},
    widgets::{Block, Borders},
    layout::Rect,
    Frame
};

use crate::app::App;

use super::{list::List, utils::{render_editing_list, render_list}};

/// The border set of the current column.
fn current_border_set() -> Set<'static> {
    Set {
        top_left: line::NORMAL.horizontal_down,
        bottom_left: line::NORMAL.horizontal_up,
        ..border::ROUNDED
    }
}

pub fn render_current(app: &mut App, frame: &mut Frame, area: Rect) {
    let current_block = Block::default()
        .border_set(current_border_set())
        .borders(
            if app.root() {
                Borders::ALL
            } else {
                Borders::TOP | Borders::BOTTOM | Borders::LEFT
            }
        );

    let marked_items = if app.root() {
        if let Some(file) = app.get_file_saver() {
            app.marked_files.get(
                &app.path.join(&file.name)
            )
        } else {
            None
        }
    } else {
        app.marked_files.get(&app.path)
    };

    let (current_items, marked) = if app.edit_mode.enabled {
        render_editing_list(
            &app.edit_mode,
            &app.current_files,
            &app.term_colors
        )
    } else {
        render_list(
            app.current_files.iter(),
            &app.term_colors,
            marked_items
        )   
    };

    frame.render_stateful_widget(
        List::new(
            current_block,
            current_items,
            marked,
            app.edit_mode.inserting()
        )
            .index(
                app.navi_index.show() && !app.root(),
                app.navi_index.index(),
                app.term_colors.executable_style
            ),
        area,
        &mut app.selected_item.current
    );
}
