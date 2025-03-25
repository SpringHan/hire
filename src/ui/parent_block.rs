// Parent Block.

use ratatui::{layout::Rect, widgets::{Block, Borders, List}, Frame};

use crate::app::{App, FileOperation};

use super::utils::render_list;

pub fn render_parent(app: &mut App, frame: &mut Frame, area: Rect) {
    let parent_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM);
    let parent_items = render_list(
        app.parent_files.iter(),
        app.selected_item.parent_selected(),
        app.move_index,
        &app.term_colors,
        None,
        FileOperation::None
    );

    frame.render_stateful_widget(
        List::new(parent_items).block(parent_block),
        area,
        &mut app.selected_item.parent
    );
}
