// Child Block

use anyhow::bail;
use ratatui_image::{StatefulImage, Resize};

use ratatui::{
    buffer::CellDiffOption,
    symbols::{border::{Set, PLAIN}, line},
    widgets::{Block, Borders},
    layout::Rect,
    text::Text,
    Frame
};

use crate::{app::App, utils::{update_window_height, FileContent}};

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

    let marked_files = if app.root() {
        None
    } else {
        if let Some(file) = app.get_file_saver() {
            app.marked_files.get(&app.path.join(&file.name))
        } else {
            None
        }
    };

    let (child_items, marked) = render_list(
        app.child_files.iter(),
        &app.term_colors,
        marked_files
    );

    frame.render_stateful_widget(
        List::new(child_block, child_items, marked, false),
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
            let inner = block.inner(layout);

            frame.render_stateful_widget(
                StatefulImage::default().resize(Resize::Fit(None)),
                inner,
                protocol
            );

            pin_image_cells(frame, inner);
            frame.render_widget(block, layout);

            return Ok(())
        }

        bail!("Failed to get image protocol of current image")
    }

    frame.render_widget(content_para(app)?, block.inner(layout));
    frame.render_widget(block, layout);

    Ok(())
}

/// Give the cells which carry an image escape sequence a width of one column.
///
/// `ratatui`'s `Buffer::diff` derives a cell's width from its symbol. The
/// image protocols (kitty, iTerm2, sixel) put the whole image row — cursor
/// movements and unicode placeholders included — into the symbol of the first
/// cell of the row. Without an explicit width that symbol looks hundreds of
/// columns wide, so the diff skips the rest of the screen: the image is only
/// partially drawn and the following frames/layout get out of sync.
/// `CellDiffOption::ForcedWidth(1)` is the escape hatch designed for exactly
/// this case.
fn pin_image_cells(frame: &mut Frame, area: Rect) {
    let one = core::num::NonZeroU16::new(1).expect("1 is not zero");
    let buffer = frame.buffer_mut();

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue
            };

            if cell.symbol().contains('\x1b') {
                cell.diff_option = CellDiffOption::ForcedWidth(one);
            }
        }
    }
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
