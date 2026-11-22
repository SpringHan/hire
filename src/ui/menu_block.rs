// Menu overlay.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph,
    },
    Frame
};

use crate::{
    app::App,
    key_event::{MenuList, MenuPresentation, MenuRow},
    utils::{display_width, Block as AppBlock, CmdContent},
};

/// The blank columns between the menu content and the box border.
const PADDING: u16 = 1;

/// The two border columns, the padding and one content column.
const MIN_WIDTH: u16 = 2 + 2 * PADDING + 1;

/// The two border rows and one content row.
const MIN_HEIGHT: u16 = 3;

/// The box a menu is drawn in.
struct MenuLayout {
    rect: Rect,
    inner_width: u16,
    inner_height: u16,
}

/// Draw the open menu over the page band `area`.
///
/// The page behind the box is dimmed, so the menu reads as the active window
/// while the file list (or the shell screen) it was opened on stays visible.
pub fn render_menu(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some((title, presentation, list)) = menu_state(app) else {
        return
    };

    if let (MenuPresentation::Picker, Some(list)) = (presentation, list) {
        render_picker(app, frame, area, &title, &list);
        return
    }

    render_dialog(app, frame, area, &title);
}

/// The title, presentation and (for a picker) the rows of the open menu.
///
/// The list is copied out so the renderer can update the scroll offset on
/// `app` while drawing it.
fn menu_state(app: &App) -> Option<(String, MenuPresentation, Option<MenuList>)> {
    let case = app.switch_case.as_ref()?;

    Some((
        case.title().to_owned(),
        case.presentation(),
        case.list().cloned()
    ))
}

/// Draw a free-form text menu.
fn render_dialog(app: &mut App, frame: &mut Frame, area: Rect, title: &str) {
    let border_style = app.term_colors.dir_style;

    let text = {
        let AppBlock::CommandLine(ref content, _) = app.selected_block else {
            return
        };

        owned_text(content)
    };

    let content_width = text.lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0);
    let content_height = text.lines.len();

    let Some(layout) = frame_layout(content_width, content_height, title, area) else {
        return
    };

    let (row, column) = clamp_scroll(
        app,
        limit(content_height, layout.inner_height),
        limit(content_width, layout.inner_width)
    );

    dim_area(frame.buffer_mut(), area);

    frame.render_widget(Clear, layout.rect);
    frame.render_widget(
        Paragraph::new(text)
            .block(menu_block(border_style, title))
            .scroll((row, column)),
        layout.rect
    );
}

/// Draw a list menu: hint lines, the entries and the notices.
fn render_picker(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    title: &str,
    list: &MenuList
) {
    let key_width = list.key_width();
    let border_style = app.term_colors.dir_style;
    let key_style = app.term_colors.executable_style;
    let selected = list.selected_row();

    // The highlighted entry reverses its key only: the rest of the row stays
    // readable (a tab with shells shows their count after the path).
    let rows = list.rows()
        .iter()
        .enumerate()
        .map(|(idx, row)| match row {
            MenuRow::Line(line) => line.clone(),
            MenuRow::Entry(entry) => {
                let style = if Some(idx) == selected {
                    key_style.add_modifier(Modifier::REVERSED)
                } else {
                    key_style
                };

                list.entry_line(entry, key_width, style)
            },
        })
        .collect::<Vec<_>>();

    let content_width = list.header_lines()
        .iter()
        .chain(rows.iter())
        .chain(list.footer_lines().iter())
        .map(|line| line.width())
        .max()
        .unwrap_or(0);
    let content_height = list.header_lines().len()
        + rows.len()
        + list.footer_lines().len();

    let Some(layout) = frame_layout(content_width, content_height, title, area) else {
        return
    };

    let (stored_row, stored_column) = app.command_scroll.unwrap_or((0, 0));
    let column = stored_column.min(limit(content_width, layout.inner_width));

    let block = menu_block(border_style, title);
    let inner = block.inner(layout.rect);

    // The hint lines first, the entries in the middle and the notices (an
    // error) at the bottom; the entries get the rows which remain.
    let header_height = u16::try_from(list.header_lines().len())
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let footer_height = u16::try_from(list.footer_lines().len())
        .unwrap_or(u16::MAX)
        .min(inner.height - header_height);
    let list_height = inner.height - header_height - footer_height;

    let areas = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(list_height),
        Constraint::Length(footer_height),
    ])
        .split(inner);

    dim_area(frame.buffer_mut(), area);

    frame.render_widget(Clear, layout.rect);
    frame.render_widget(&block, layout.rect);

    if header_height > 0 {
        frame.render_widget(
            Paragraph::new(trimmed_text(list.header_lines(), column)),
            areas[0]
        );
    }

    let max_row = u16::try_from(rows.len().saturating_sub(list_height as usize))
        .unwrap_or(u16::MAX);

    if list_height > 0 && !rows.is_empty() {
        let mut state = ListState::default()
            .with_selected(list.selected_row())
            .with_offset(stored_row.min(max_row) as usize);

        let items = rows
            .into_iter()
            .map(|line| ListItem::new(trimmed_line(&line, column)))
            .collect::<Vec<_>>();

        frame.render_stateful_widget(
            List::new(items),
            areas[1],
            &mut state
        );

        // The list keeps the selection in view, so the offset it settled on
        // is the one the next `Ctrl-p`/`Ctrl-n` continues from.
        app.command_scroll = Some((
            u16::try_from(state.offset()).unwrap_or(u16::MAX),
            column
        ));
    } else {
        app.command_scroll = Some((0, column));
    }

    if footer_height > 0 {
        frame.render_widget(
            Paragraph::new(trimmed_text(list.footer_lines(), column)),
            areas[2]
        );
    }
}

/// The box every menu is drawn in, its border and title in `style`.
fn menu_block(style: Style, title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(style)
        .padding(Padding::horizontal(PADDING))
        .title_top(Line::styled(format!(" {title} "), style))
}

/// The message of a dialog as an owned `Text`.
///
/// A clone lets the message be measured and drawn after the `app` borrow which
/// produced it has ended, which is needed because the scroll offset lives on
/// `app` too.
fn owned_text(content: &CmdContent) -> Text<'static> {
    match content {
        CmdContent::Text(text) => text.clone(),
        CmdContent::String(string) => Text::raw(string.clone()),
    }
}

/// Center the box in `area`, sized to `content_width` x `content_height`.
///
/// A row and a column of the page stay visible on each side, so the box never
/// covers the whole browser; `None` is returned when not even a minimal box
/// fits.
fn frame_layout(
    content_width: usize,
    content_height: usize,
    title: &str,
    area: Rect
) -> Option<MenuLayout> {
    let max_width = area.width.saturating_sub(2);
    let max_height = area.height.saturating_sub(2);

    if max_width < MIN_WIDTH || max_height < MIN_HEIGHT {
        return None
    }

    let content_width = u16::try_from(content_width).unwrap_or(u16::MAX);
    let content_height = u16::try_from(content_height).unwrap_or(u16::MAX);

    // The title is drawn as `" {title} "`, so the box has to be wide enough
    // for the title plus those two spaces.
    let title_width = u16::try_from(display_width(title))
        .unwrap_or(u16::MAX)
        .saturating_add(2);

    let width = content_width
        .saturating_add(2 * PADDING)
        .max(title_width)
        .saturating_add(2)
        .clamp(MIN_WIDTH, max_width);

    let height = content_height
        .saturating_add(2)
        .clamp(MIN_HEIGHT, max_height);

    Some(MenuLayout {
        rect: Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height
        ),
        inner_width: width - 2 - 2 * PADDING,
        inner_height: height - 2,
    })
}

/// Clamp the stored scroll offset to what the content allows.
///
/// The box size is only known while rendering, so the offset is corrected
/// here: a menu opened after a longer one, or a menu which became shorter,
/// must not stay scrolled past its content.
fn clamp_scroll(app: &mut App, max_row: u16, max_column: u16) -> (u16, u16) {
    let (row, column) = app.command_scroll.unwrap_or((0, 0));
    let (row, column) = (row.min(max_row), column.min(max_column));

    if app.command_scroll.is_some() {
        app.command_scroll = Some((row, column));
    }

    (row, column)
}

/// The largest row/column offset of `content` inside `inner` cells.
fn limit(content: usize, inner: u16) -> u16 {
    u16::try_from(content.saturating_sub(inner as usize)).unwrap_or(u16::MAX)
}

/// Drop the first `column` display columns of `line`, for the sideways scroll.
fn trimmed_line(line: &Line<'static>, column: u16) -> Line<'static> {
    if column == 0 {
        return line.clone()
    }

    let mut remaining = column as usize;
    let mut spans: Vec<Span<'static>> = Vec::new();

    for span in &line.spans {
        let mut content = String::new();

        for ch in span.content.chars() {
            if remaining == 0 {
                content.push(ch);
                continue
            }

            let width = display_width(&ch.to_string());

            // A zero-width mark belongs to the character before it; when that
            // character is scrolled out, the mark goes with it.
            if width == 0 {
                continue
            }

            if width <= remaining {
                remaining -= width;
            } else {
                // The cut falls inside a wide character: drop it.
                remaining = 0;
            }
        }

        if !content.is_empty() {
            spans.push(Span::styled(content, span.style));
        }
    }

    let mut trimmed = line.clone();
    trimmed.spans = spans;

    trimmed
}

fn trimmed_text(lines: &[Line<'static>], column: u16) -> Text<'static> {
    Text::from(
        lines
            .iter()
            .map(|line| trimmed_line(line, column))
            .collect::<Vec<_>>()
    )
}

/// Dim every cell of `area`, so the page behind the menu recedes.
fn dim_area(buf: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(cell.style().add_modifier(Modifier::DIM));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier, Style},
        Terminal
    };

    use crate::error::AppResult;
    use crate::key_event::{
        MenuList, MenuPresentation, SwitchCase, SwitchCaseData,
    };
    use crate::utils::CmdContent;

    use super::*;

    fn stub(_: &mut App, _: char, _: SwitchCaseData) -> AppResult<bool> {
        Ok(true)
    }

    fn open_dialog(app: &mut App, title: &str, msg: CmdContent) {
        SwitchCase::new(
            app,
            stub,
            MenuPresentation::Dialog,
            title,
            msg,
            SwitchCaseData::None
        );
    }

    fn open_picker(app: &mut App, title: &str, list: MenuList) {
        SwitchCase::new(
            app,
            stub,
            MenuPresentation::Picker,
            title,
            list,
            SwitchCaseData::None
        );
    }

    fn draw(app: &mut App, area: Rect) -> Buffer {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_menu(app, frame, area))
            .unwrap();

        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &Buffer, y: u16, x: u16, width: u16) -> String {
        (x..x + width)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect()
    }

    /// The dialog is a centered box: rounded corners, its title on the top
    /// border, and the page around it dimmed while the box itself is not.
    #[test]
    fn test_dialog_is_a_centered_box_with_its_title() {
        let mut app = App::default();
        open_dialog(
            &mut app,
            "Delete",
            CmdContent::String(String::from("one\ntwo"))
        );

        let area = Rect::new(0, 0, 40, 12);
        let layout = frame_layout(3, 2, "Delete", area).unwrap();
        let buffer = draw(&mut app, area);
        let rect = layout.rect;

        assert_eq!(buffer[(rect.x, rect.y)].symbol(), "╭");
        assert_eq!(buffer[(rect.right() - 1, rect.y)].symbol(), "╮");
        assert_eq!(buffer[(rect.x, rect.bottom() - 1)].symbol(), "╰");
        assert_eq!(
            buffer[(rect.right() - 1, rect.bottom() - 1)].symbol(),
            "╯"
        );

        let top = row_text(&buffer, rect.y, rect.x, rect.width);
        assert!(top.contains("Delete"), "{top}");

        // The page around the box is dimmed; the box is drawn over it.
        assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
        assert!(
            !buffer[(rect.x, rect.y)].modifier.contains(Modifier::DIM)
        );
    }

    /// A picker aligns the keys in their own column and reverses only the key
    /// of the selected entry.
    #[test]
    fn test_picker_aligns_keys_and_highlights_the_selected_key() {
        let mut app = App::default();
        open_picker(
            &mut app,
            "Test",
            MenuList::new()
                .header(Line::raw("hints"))
                .entry("1", Line::raw("first"))
                .entry("10", Line::raw("second"))
                .select(Some(1))
        );

        let area = Rect::new(0, 0, 40, 15);
        let buffer = draw(&mut app, area);

        // `[ 1] first` / `[10] second`: the keys share one column.
        let first_row = (0..area.height)
            .find(|y| row_text(&buffer, *y, 0, area.width).contains("[ 1] first"))
            .expect("the first entry is drawn");
        let second_row = (0..area.height)
            .find(|y| row_text(&buffer, *y, 0, area.width).contains("[10] second"))
            .expect("the second entry is drawn");

        let key_x = (0..area.width)
            .find(|x| buffer[(*x, first_row)].symbol() == "[")
            .unwrap();
        assert_eq!(buffer[(key_x, second_row)].symbol(), "[");

        // Only the selected key is reversed; the rest of its row stays
        // readable (a tab with shells prints their count after the path).
        let text_x = key_x + "[ 1] ".len() as u16;

        assert!(
            buffer[(key_x, second_row)].modifier.contains(Modifier::REVERSED)
        );
        assert!(
            !buffer[(text_x, second_row)].modifier.contains(Modifier::REVERSED)
        );
        assert!(
            !buffer[(key_x, first_row)].modifier.contains(Modifier::REVERSED)
        );
    }

    /// The border and the title take the directory colour, so the box stands
    /// out from the dimmed page.
    #[test]
    fn test_menu_border_and_title_use_the_directory_style() {
        let mut app = App::default();
        app.term_colors.dir_style = Style::new().fg(Color::Blue);

        open_dialog(
            &mut app,
            "Delete",
            CmdContent::String(String::from("one"))
        );

        let area = Rect::new(0, 0, 40, 12);
        let layout = frame_layout(3, 1, "Delete", area).unwrap();
        let buffer = draw(&mut app, area);
        let rect = layout.rect;

        assert_eq!(buffer[(rect.x, rect.y)].fg, Color::Blue);
        assert_eq!(buffer[(rect.x, rect.bottom() - 1)].fg, Color::Blue);

        let title_x = (rect.left()..rect.right())
            .find(|x| buffer[(*x, rect.y)].symbol() == "D")
            .expect("the title is drawn");
        assert_eq!(buffer[(title_x, rect.y)].fg, Color::Blue);
    }

    /// The key column uses the executable colour, and the selected key keeps
    /// that colour reversed.
    #[test]
    fn test_picker_keys_use_the_executable_style() {
        let mut app = App::default();
        app.term_colors.executable_style = Style::new().fg(Color::Green);

        open_picker(
            &mut app,
            "Test",
            MenuList::new()
                .entry("1", Line::raw("first"))
                .entry("2", Line::raw("second"))
                .select(Some(1))
        );

        let area = Rect::new(0, 0, 40, 12);
        let buffer = draw(&mut app, area);

        let first_row = (0..area.height)
            .find(|y| row_text(&buffer, *y, 0, area.width).contains("[1] first"))
            .unwrap();
        let second_row = (0..area.height)
            .find(|y| row_text(&buffer, *y, 0, area.width).contains("[2] second"))
            .unwrap();

        let key_x = (0..area.width)
            .find(|x| buffer[(*x, first_row)].symbol() == "[")
            .unwrap();

        assert_eq!(buffer[(key_x, first_row)].fg, Color::Green);
        assert_eq!(buffer[(key_x, second_row)].fg, Color::Green);
        assert!(
            buffer[(key_x, second_row)].modifier.contains(Modifier::REVERSED)
        );
        assert!(
            !buffer[(key_x, first_row)].modifier.contains(Modifier::REVERSED)
        );
    }

    /// A list taller than the box scrolls inside it; the offset is clamped and
    /// a selected entry is scrolled back into view.
    #[test]
    fn test_picker_scrolls_and_follows_the_selection() {
        let mut app = App::default();
        let mut rows = MenuList::new();

        for idx in 0..40 {
            rows = rows.entry((idx + 1).to_string(), Line::raw(format!("line {idx}")));
        }

        open_picker(&mut app, "Long", rows);

        let area = Rect::new(0, 0, 40, 12);
        app.command_scroll = Some((99, 0));
        let buffer = draw(&mut app, area);

        // 40 rows in a box whose inner height is 8: the last 8 are shown.
        assert_eq!(app.command_scroll, Some((32, 0)));

        let bottom = row_text(&buffer, area.height - 3, 0, area.width);
        assert!(bottom.contains("line 39"), "{bottom}");

        // Selecting the first entry scrolls back to the top.
        let list = MenuList::new()
            .entry("1", Line::raw("line 0"))
            .entry("2", Line::raw("line 1"))
            .select(Some(0));
        open_picker(&mut app, "Long", list);

        app.command_scroll = Some((32, 0));
        let _ = draw(&mut app, area);

        assert_eq!(app.command_scroll, Some((0, 0)));
    }

    /// An error notice is drawn below the entries and does not push them out
    /// of the box.
    #[test]
    fn test_picker_draws_a_notice() {
        let mut app = App::default();
        open_picker(
            &mut app,
            "Test",
            MenuList::new().entry("1", Line::raw("first"))
        );

        app.switch_case
            .as_mut()
            .unwrap()
            .append_notice(String::from("cannot attach"));

        let area = Rect::new(0, 0, 40, 12);
        let buffer = draw(&mut app, area);

        let text: String = (0..area.height)
            .map(|y| row_text(&buffer, y, 0, area.width))
            .collect();
        assert!(text.contains("cannot attach"), "{text}");
        assert!(text.contains("first"), "{text}");
    }

    /// A page too small for even a minimal box is left alone: dimming it
    /// without a menu on top would hide the browser.
    #[test]
    fn test_menu_needs_a_minimal_page() {
        let area = Rect::new(0, 0, 4, 4);
        assert!(frame_layout(1, 1, "M", area).is_none());
    }
}
