// Current Block

use ratatui::{
    symbols::{border::{Set, PLAIN}, line},
    widgets::{Block, Borders},
    layout::Rect,
    Frame
};

use crate::app::App;

use super::{list::List, utils::render_list};

pub fn render_current(app: &mut App, frame: &mut Frame, area: Rect) {
    let border_set = Set {
        top_left: line::NORMAL.horizontal_down,
        bottom_left: line::NORMAL.horizontal_up,
        ..PLAIN
    };
    let current_block = Block::default()
        .border_set(border_set)
        .borders(Borders::TOP | Borders::BOTTOM | Borders::LEFT);

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

    let (current_items, marked) = render_list(
        app.current_files.iter(),
        &app.term_colors,
        marked_items
    );

    frame.render_stateful_widget(
        List::new(current_block, current_items, marked)
            .index(
                app.navi_index.show() && !app.root(),
                app.navi_index.index(),
                app.term_colors.executable_style
            ),
        area,
        &mut app.selected_item.current
    );
}
