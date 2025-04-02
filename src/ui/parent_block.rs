// Parent Block.

use std::path::PathBuf;

use ratatui::{layout::Rect, widgets::{Block, Borders}, Frame};

use crate::app::App;

use super::{list::List, utils::render_list};

pub fn render_parent(app: &mut App, frame: &mut Frame, area: Rect) {
    let parent_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM);

    let marked_files = if app.root() {
        app.marked_files.get(&PathBuf::from("/"))
    } else {
        if let Some(parent) = app.path.parent() {
            app.marked_files.get(parent)
        } else {
            None
        }
    };

    let (parent_items, marked) = render_list(
        app.parent_files.iter(),
        &app.term_colors,
        marked_files
    );

    frame.render_stateful_widget(
        List::new(parent_block, parent_items, marked, false)
            .index(
                app.navi_index.show() && app.root(),
                app.navi_index.index(),
                app.term_colors.executable_style
            ),
        area,
        &mut app.selected_item.parent
    );
}
