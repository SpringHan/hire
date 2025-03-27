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

    let marked_items = if app.path.to_string_lossy() == "/" {
        let path = app.path.join(
            &app.get_file_saver().unwrap().name
        );
        app.marked_files.get(&path)
    } else {
        app.marked_files.get(&app.path)
    };

    let current_items = render_list(
        app.current_files.iter(),
        &app.term_colors,
        marked_items,
        app.marked_operation
    );

    frame.render_stateful_widget(
        List::new(current_block, current_items)
            .index(
                app.navi_index.show() && !app.root(),
                app.navi_index.index(),
                app.term_colors.executable_style
            ),
        area,
        &mut app.selected_item.current
    );
}
