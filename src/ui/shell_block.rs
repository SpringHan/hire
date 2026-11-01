// Shell view.
//
// Renders the `vt100` screen of the attached shell and its status line.

use std::path::PathBuf;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    Frame,
};

use crate::app::App;

use super::utils::shorten_path;

const CWD_MAX_LEN: usize = 40;

/// Draw the attached shell's screen into `area`.
///
/// The PTY and the parser are resized to the area first, so the shell sees the
/// same dimensions it is drawn with.
pub fn render_shell(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
) -> anyhow::Result<()>
{
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };
    let Some(session) = app.shell_sessions.get_mut(id) else {
        return Ok(())
    };

    session
        .resize(area.height, area.width)
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    let parser = session.parser();
    let screen = parser.screen();

    {
        let buf = frame.buffer_mut();

        for row in 0..area.height {
            for col in 0..area.width {
                let Some(cell) = screen.cell(row, col) else {
                    continue
                };

                // The leading cell of a wide character already covers the
                // continuation cell.
                if cell.is_wide_continuation() {
                    continue
                }

                if let Some(target) = buf.cell_mut((area.x + col, area.y + row)) {
                    // `vt100` returns an empty string for a blank cell. Writing
                    // an empty symbol makes crossterm print nothing, so the
                    // character which used to be there would stay on screen.
                    // A real space must be written to erase it.
                    if cell.has_contents() {
                        target.set_symbol(&cell.contents());
                    } else {
                        target.set_symbol(" ");
                    }

                    target.set_style(cell_style(cell));
                }
            }
        }
    }

    if !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();

        if row < area.height && col < area.width {
            frame.set_cursor_position((area.x + col, area.y + row));
        }
    }

    Ok(())
}

/// The left and right line of the shell status bar.
pub fn shell_status_lines(app: &App) -> Vec<Line<'static>> {
    let Some(id) = app.selected_block.shell_id() else {
        return Vec::new()
    };
    let Some(session) = app.shell_sessions.get(id) else {
        return Vec::new()
    };

    let tab = app.tab_list.current_tab();
    let total = tab.shells.len();
    let pos = tab
        .shells
        .iter()
        .position(|shell| *shell == id)
        .map(|idx| idx + 1)
        .unwrap_or(0);

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cwd = shorten_path(&session.meta.cwd, home.as_deref(), CWD_MAX_LEN);

    let left = Line::from(vec![
        Span::styled(
            session.display_title(),
            app.term_colors.executable_style,
        )
        .add_modifier(Modifier::BOLD),
        Span::raw(format!("  {cwd}  [{pos}/{total}]")),
    ]);

    let (hint, style) = if let Some(pending) = app.shell_pending_kill {
        let title = app
            .shell_sessions
            .get(pending)
            .map(|session| session.display_title())
            .unwrap_or_else(|| String::from("shell"));

        (
            format!("Kill shell \"{title}\"? (y/N)"),
            Style::default().red().add_modifier(Modifier::BOLD),
        )
    } else if !app.shell_digits.is_empty() {
        (
            format!(
                "Shell #{}  (Enter: attach, Esc: cancel)",
                app.shell_digits
            ),
            Style::default().yellow().add_modifier(Modifier::BOLD),
        )
    } else if app.shell_prefix_active {
        (
            String::from("PREFIX  d: detach  n/p: switch  c: new  x: kill"),
            Style::default().cyan().add_modifier(Modifier::BOLD),
        )
    } else if let Some(notice) = app.shell_notice.as_ref() {
        (
            notice.clone(),
            Style::default().red().add_modifier(Modifier::BOLD),
        )
    } else {
        (
            String::from(
                "prefix d: detach  n/p: switch  c: new  x: kill  prefix x2: literal",
            ),
            Style::default().add_modifier(Modifier::DIM),
        )
    };

    let right = Line::from(Span::styled(hint, style))
        .alignment(Alignment::Right);

    vec![left, right]
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::default()
        .fg(to_color(cell.fgcolor()))
        .bg(to_color(cell.bgcolor()));

    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }

    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }

    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }

    style
}

fn to_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
