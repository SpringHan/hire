// Child Block

use std::borrow::Cow;

use anyhow::bail;
use ansi_to_tui::IntoText;
use ratatui_image::thread::ThreadImage;
use ratatui::{
    widgets::{Block, Borders, Clear, Padding, Paragraph},
    layout::Rect,
    text::Text,
    Frame
};

use crate::app::{App, FileContent};

pub fn render_file(frame: &mut Frame, app: &mut App, layout: Rect) -> anyhow::Result<()> {
    frame.render_widget(Clear, layout);
    let block = Block::default()
        .borders(Borders::NONE)
        .padding(Padding::right(1));

    if app.file_content == FileContent::Image {
        let _ref = app.image_preview.image_protocol();

        if let Some(protocol) = _ref {
            frame.render_stateful_widget(
                ThreadImage::default(),
                block.inner(layout),
                protocol
            );

            return Ok(())
        }

        bail!("Failed to get image protocol of current image")
    }

    frame.render_widget(content_para(app, block)?, layout);

    Ok(())
}

/// Wrap current file content with Pragraph Widget.
fn content_para<'a>(
    app: &'a App,
    file_block: Block<'a>
) -> anyhow::Result<Paragraph<'a>>
{
    if let FileContent::Text(ref content) = app.file_content {
        Ok(
            Paragraph::new(text_to_render(content)?)
                .style(app.term_colors.file_style)
                .block(file_block)
        )
    } else {
        panic!("Unknown error occurred when rendering file content!")
    }
}

/// The function to solve the issue that `Paragraph` cannot render '\t' properly.
fn text_to_render(text: &String) -> anyhow::Result<Text<'_>> {
    let mut after_fmt = text.into_text()?;

    for line in after_fmt.iter_mut() {
        for span in line.iter_mut() {
            span.content = Cow::Owned(span.content.replace("\t", "    "));
        }
    }

    Ok(after_fmt)
}
