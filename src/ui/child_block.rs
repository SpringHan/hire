// Child Block

use anyhow::bail;
use ratatui_image::{thread::ThreadImage, Resize};

use ratatui::{
    symbols::{border::{Set, PLAIN}, line},
    widgets::{Block, Borders},
    layout::Rect,
    text::Text,
    Frame
};

use crate::{app::{App, FileContent, FileOperation}, utils::update_window_height};

use super::{list::List, utils::render_list};

pub fn render_child(app: &mut App, frame: &mut Frame, area: Rect) {
    let border_set = Set {
        top_left: line::NORMAL.horizontal_down,
        bottom_left: line::NORMAL.horizontal_up,
        ..PLAIN
    };
    let child_block = Block::default()
        .border_set(border_set)
        .borders(Borders::ALL);

    // Update file linenr
    update_file_linenr(child_block.inner(area));

    let child_items = render_list(
        app.child_files.iter(),
        &app.term_colors,
        None,
        FileOperation::None
    );

    frame.render_stateful_widget(
        List::new(child_block, child_items),
        area,
        &mut app.selected_item.child
    );
}

pub fn render_file(frame: &mut Frame, app: &mut App, layout: Rect) -> anyhow::Result<()> {
    let border_set = Set {
        top_left: line::NORMAL.horizontal_down,
        bottom_left: line::NORMAL.horizontal_up,
        ..PLAIN
    };
    let block = Block::default()
        .border_set(border_set)
        .borders(Borders::ALL);

    update_file_linenr(block.inner(layout));

    if app.file_content == FileContent::Image {
        let _ref = app.image_preview.image_protocol();

        if let Some(protocol) = _ref {
            frame.render_stateful_widget(
                ThreadImage::default().resize(Resize::Fit(None)),
                block.inner(layout),
                protocol
            );
            frame.render_widget(block, layout);

            return Ok(())
        }

        bail!("Failed to get image protocol of current image")
    }

    frame.render_widget(content_para(app)?, block.inner(layout));
    frame.render_widget(block, layout);

    Ok(())
}

pub fn update_file_linenr(area: Rect) {
    update_window_height(area.height);
}

/// Wrap current file content with Pragraph Widget.
fn content_para<'a>(app: &'a App,) -> anyhow::Result<&'a Text<'a>> {
    if let FileContent::Text(ref content) = app.file_content {
        Ok(content)
    } else {
        bail!("Unknown error occurred when rendering file content!")
    }
}
