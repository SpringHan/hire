// Child Block

use anyhow::bail;
use ratatui::{layout::Rect, widgets::{Block, Borders, Paragraph}, Frame};
use ratatui_image::StatefulImage;

use crate::app::{App, FileContent};

pub fn render_file(frame: &mut Frame, app: &mut App, layout: Rect) -> anyhow::Result<()> {
    if app.file_content == FileContent::Image {
        let protocol = app.image_preview.image_protocol();
        if protocol.is_none() {
            bail!("Failed to get image protocol of current image")
        }
        
        frame.render_stateful_widget(
            StatefulImage::default(),
            layout,
            protocol.unwrap()
        );

        return Ok(())
    }

    frame.render_widget(content_para(app), layout);

    Ok(())
}

/// Wrap current file content with Pragraph Widget.
fn content_para(app: &App) -> Paragraph {
    let file_block = Block::default()
        .borders(Borders::ALL);
    if let FileContent::Text(ref content) = app.file_content {
        Paragraph::new(content.as_str())
            .style(app.term_colors.file_style)
            .block(file_block)
    } else {
        panic!("Unknown error occurred when rendering file content!")
    }
}
