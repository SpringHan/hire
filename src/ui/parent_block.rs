// Parent Block.

use ratatui::{layout::Rect, widgets::{Block, Borders}, Frame};

use crate::app::{App, FileOperation};

use super::{list::List, utils::render_list};

pub fn render_parent(app: &mut App, frame: &mut Frame, area: Rect) {
    let parent_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM);
    let parent_items = render_list(
        app.parent_files.iter(),
        &app.term_colors,
        None,
        FileOperation::None
    );

    frame.render_stateful_widget(
        List::new(parent_block, parent_items)
            .index(
                app.navi_index.show() && app.root(),
                app.navi_index.index(),
                app.term_colors.executable_style
            ),
        area,
        &mut app.selected_item.parent
    );
}
