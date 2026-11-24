//! The child column and the file preview, plus the right edge of the frame.

use anyhow::bail;
use ratatui_image::{StatefulImage, Resize};

use ratatui::{
    buffer::CellDiffOption,
    symbols::{border::{self, Set}, line},
    widgets::{Block, Borders, Paragraph},
    layout::Rect,
    style::Modifier,
    text::{Line, Span, Text},
    Frame
};

use crate::{
    app::App,
    utils::{update_window_height, update_window_width, FileContent}
};

use super::{list::List, utils::render_list};

/// The border set of the columns which close the browser's rounded frame on
/// the right: the child column and the file preview.
fn right_end_border_set() -> Set<'static> {
    Set {
        top_left: line::NORMAL.horizontal_down,
        bottom_left: line::NORMAL.horizontal_up,
        ..border::ROUNDED
    }
}

pub fn render_child(app: &mut App, frame: &mut Frame, area: Rect) {
    let child_block = Block::default()
        .border_set(right_end_border_set())
        .borders(Borders::ALL);

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
    let block = Block::default()
        .border_set(right_end_border_set())
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

    let inner = block.inner(layout);

    // A window read for another pane size (or an offset it does not cover) is
    // reloaded before it is drawn, so a resize keeps the page full.
    app.ensure_preview_window(inner.height);

    let column = u16::try_from(app.preview.column()).unwrap_or(u16::MAX);

    // `Paragraph` is what shifts the lines horizontally; the vertical window is
    // already cut by `preview_lines`.
    frame.render_widget(
        Paragraph::new(Text::from(preview_lines(app, inner.height)?))
            .scroll((0, column)),
        inner
    );
    frame.render_widget(block, layout);

    Ok(())
}

/// Give the cells which carry an image escape sequence a width of one column.
///
/// `Buffer::diff` derives a cell's width from its symbol, and the image
/// protocols (kitty, iTerm2, sixel) put a whole image row — cursor movements
/// and unicode placeholders included — into the symbol of the first cell of
/// the row. Without an explicit width that symbol looks hundreds of columns
/// wide, so the diff skips the rest of the screen: the image is partly drawn
/// and the next frames get out of sync; `CellDiffOption::ForcedWidth(1)` is
/// the escape hatch for exactly this.
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
    update_window_width(area.width);
}

/// The lines of the text preview which fit in `height` rows.
///
/// The preview holds a window of the file, so the rendered lines start at
/// `offset - window_start` inside it; paging and searching move that offset,
/// and the window is reloaded only when the page leaves it. Every occurrence
/// of the search query is drawn reversed, which keeps the match visible
/// whatever the syntax highlighting painted there; the match the search or
/// `n`/`N` moved to is underlined as well, so it stands out from the others.
fn preview_lines<'a>(
    app: &'a App,
    height: u16
) -> anyhow::Result<Vec<Line<'a>>>
{
    let FileContent::Text(ref content) = app.file_content else {
        bail!("Unknown error occurred when rendering file content!")
    };

    let total = content.lines.len();
    let window_start = app.preview.window_start();
    // The window may hold fewer lines than the offset expects (a file which
    // shrank while it was open): clamp to what it has instead of slicing past
    // the end.
    let start = app.preview.offset().saturating_sub(window_start).min(total);
    let end = start.saturating_add(height as usize).min(total);

    let current = app.preview.current_line();

    Ok(content.lines[start..end]
        .iter()
        .enumerate()
        .map(|(row, line)| {
            highlight_matches(
                line,
                &app.preview.match_ranges(line),
                current == Some(window_start + start + row)
            )
        })
        .collect())
}

/// Reverse the matched character ranges of `line`.
///
/// The spans are split at the range boundaries, so only the matched characters
/// are reversed and the rest keeps its syntax colours. `current` is the line
/// the search or `n`/`N` moved to: its matches are underlined too, which is
/// what tells the selected occurrence apart from the other ones.
fn highlight_matches<'a>(
    line: &Line<'a>,
    ranges: &[(usize, usize)],
    current: bool
) -> Line<'a>
{
    if ranges.is_empty() {
        return line.to_owned()
    }

    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut char_idx = 0;

    for span in &line.spans {
        let mut run = String::new();
        let mut run_matched = false;

        // The span's own style, reversed (and underlined on the current line)
        // when the run it belongs to matched.
        let styled = |content: String, matched: bool| {
            let mut style = span.style;

            if matched {
                style = style.add_modifier(Modifier::REVERSED);

                if current {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
            }

            Span::styled(content, style)
        };

        for ch in span.content.chars() {
            let matched = ranges.iter().any(|(start, end)| {
                char_idx >= *start && char_idx < *end
            });

            // The matched flag changed: the characters collected so far keep
            // the style of their own range.
            if !run.is_empty() && matched != run_matched {
                spans.push(styled(std::mem::take(&mut run), run_matched));
            }

            run_matched = matched;
            run.push(ch);
            char_idx += 1;
        }

        if !run.is_empty() {
            spans.push(styled(run, run_matched));
        }
    }

    let mut highlighted = line.to_owned();
    highlighted.spans = spans;

    highlighted
}

#[cfg(test)]
mod test {
    use ratatui::style::Color;

    use super::*;

    /// Only the matched characters are reversed; on the current match line
    /// they are underlined too.
    #[test]
    fn test_highlight_matches_splits_the_spans() {
        let style = ratatui::style::Style::new().fg(Color::Red);
        let line = Line::from(vec![
            Span::styled("hello ", style),
            Span::styled("needle", style),
            Span::styled(" world", style),
        ]);

        let highlighted = highlight_matches(&line, &[(6, 12)], false);

        let content: String = highlighted.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(content, "hello needle world");

        let reversed: Vec<&str> = highlighted.spans
            .iter()
            .filter(|span| {
                span.style.add_modifier.contains(Modifier::REVERSED)
            })
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(reversed, vec!["needle"]);

        // The matches of the other lines are only reversed.
        assert!(
            highlighted.spans.iter().all(|span| {
                !span.style.add_modifier.contains(Modifier::UNDERLINED)
            })
        );

        let highlighted = highlight_matches(&line, &[(6, 12)], true);
        let underlined: Vec<&str> = highlighted.spans
            .iter()
            .filter(|span| {
                span.style.add_modifier.contains(Modifier::UNDERLINED)
            })
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(underlined, vec!["needle"]);

        // Without a match the line is returned as it is.
        assert_eq!(
            highlight_matches(&line, &[], true).spans.len(),
            line.spans.len()
        );

        // A match which covers the whole line reverses all of it.
        let highlighted = highlight_matches(&line, &[(0, 18)], true);
        assert!(
            highlighted.spans.iter().all(|span| {
                span.style.add_modifier.contains(Modifier::REVERSED)
            })
        );
    }
}
