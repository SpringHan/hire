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

use crate::{
    app::App,
    key_event::ScrollCommand,
    utils::ShellId,
};

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
                        target.set_symbol(cell.contents());
                    } else {
                        target.set_symbol(" ");
                    }

                    target.set_style(cell_style(cell));
                }
            }
        }
    }

    // While the history is shown the child's cursor is somewhere on the live
    // screen, not in the viewport; drawing it there would be wrong.
    if !screen.hide_cursor() && screen.scrollback() == 0 {
        let (row, col) = screen.cursor_position();

        if row < area.height && col < area.width {
            frame.set_cursor_position((area.x + col, area.y + row));
        }
    }

    Ok(())
}

/// The left and right line of the shell status bar.
///
/// The left line names the shell; the right one only appears when it has
/// something to say — a pending prompt, typed digits or the prefix hint. The
/// idle key list is deliberately absent: the keys are shown once the user has
/// pressed `shell_prefix`, so an attached shell looks like a plain terminal
/// until then.
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

    let mut left = Line::from(vec![
        Span::styled(
            session.display_title(),
            app.term_colors.executable_style,
        )
        .add_modifier(Modifier::BOLD),
        Span::raw(format!("  {cwd}  [{pos}/{total}]")),
    ]);

    // The pinned shell is the one `C-s b` attaches from anywhere.
    if app.pinned_shell == Some(id) {
        left.push_span(
            Span::raw("  [pinned]")
                .yellow()
                .add_modifier(Modifier::BOLD),
        );
    }

    let hint = if let Some(tab_id) = app.shell_pending_kill_all {
        let count = app
            .tab_list
            .tab_by_id(tab_id)
            .map(|tab| tab.shells.len())
            .unwrap_or(0);

        Some((
            format!("Kill all {count} shell(s) of this tab? (y/N)"),
            Style::default().red().add_modifier(Modifier::BOLD),
        ))
    } else if app.shell_pending_kill_others.is_some() {
        Some((
            format!(
                "Kill {} other shell(s)? (y/N)",
                total.saturating_sub(1)
            ),
            Style::default().red().add_modifier(Modifier::BOLD),
        ))
    } else if let Some(pending) = app.shell_pending_kill {
        let title = app
            .shell_sessions
            .get(pending)
            .map(|session| session.display_title())
            .unwrap_or_else(|| String::from("shell"));

        Some((
            format!("Kill shell \"{title}\"? (y/N)"),
            Style::default().red().add_modifier(Modifier::BOLD),
        ))
    } else if !app.shell_digits.is_empty() {
        Some((
            format!(
                "Shell #{}  (Enter: attach, Esc: cancel)",
                app.shell_digits
            ),
            Style::default().yellow().add_modifier(Modifier::BOLD),
        ))
    } else if app.shell_scroll_mode {
        Some((
            scroll_hint(app, session_offset(app, id)),
            Style::default().yellow().add_modifier(Modifier::BOLD),
        ))
    } else if app.shell_prefix_active {
        Some((
            String::from(
                "PREFIX  d: detach  n/p: switch  c: new  x: kill  \
                 D: all  o: others  t: menu  *: pin  b: pinned  \
                 [: scroll",
            ),
            Style::default().cyan().add_modifier(Modifier::BOLD),
        ))
    } else {
        app.shell_notice.as_ref().map(|notice| {
            (
                notice.clone(),
                Style::default().red().add_modifier(Modifier::BOLD),
            )
        })
    };

    let mut lines = vec![left];

    if let Some((hint, style)) = hint {
        lines.push(
            Line::from(Span::styled(hint, style)).alignment(Alignment::Right),
        );
    }

    lines
}

/// How many lines above the live screen the attached shell is scrolled.
fn session_offset(app: &App, id: ShellId) -> usize {
    app.shell_sessions
        .get(id)
        .map(|session| session.parser().screen().scrollback())
        .unwrap_or(0)
}

/// The scroll mode's status hint, built from the configured bindings so a
/// remapped key is shown instead of the default one.
fn scroll_hint(app: &App, offset: usize) -> String {
    let label = |command| app.keymap.shell_scroll_label(command);
    let mut parts = vec![format!("SCROLL {offset}")];

    if let (Some(up), Some(down)) =
        (label(ScrollCommand::Up), label(ScrollCommand::Down))
    {
        parts.push(format!("{up}/{down}: line"));
    }

    if let (Some(up), Some(down)) =
        (label(ScrollCommand::PageUp), label(ScrollCommand::PageDown))
    {
        parts.push(format!("{up}/{down}: page"));
    }

    if let (Some(top), Some(bottom)) =
        (label(ScrollCommand::Top), label(ScrollCommand::Bottom))
    {
        parts.push(format!("{top}/{bottom}: top/bottom"));
    }

    if let Some(quit) = label(ScrollCommand::Quit) {
        parts.push(format!("{quit}: back"));
    }

    parts.join("  ")
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

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::{key_event::GraphicsSupport, utils::Block};

    use super::*;

    fn attach_test_shell(app: &mut App) {
        let tab_id = app.tab_list.current_tab().id;
        let id = app
            .shell_sessions
            .open(
                tab_id,
                "/bin/sh",
                Path::new("/"),
                GraphicsSupport::disabled(),
            )
            .unwrap();

        let tab = app.tab_list.current_tab_mut();
        tab.shells.push(id);
        tab.active_shell = Some(id);
        app.selected_block = Block::Shell(id);
    }

    /// A pinned shell is marked in the status line, so the one `C-s b`
    /// attaches from anywhere is recognizable at a glance.
    #[test]
    fn test_status_line_marks_the_pinned_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        attach_test_shell(&mut app);
        let id = app.selected_block.shell_id().unwrap();

        let left = |app: &App| shell_status_lines(app)[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(!left(&app).contains("[pinned]"));

        app.pinned_shell = Some(id);

        assert!(left(&app).contains("[pinned]"));
    }

    /// The idle status line has no key hint; pressing the prefix shows it.
    #[test]
    fn test_key_hint_only_appears_after_the_prefix() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        attach_test_shell(&mut app);

        // Idle: just the left line (title, cwd, position).
        assert_eq!(shell_status_lines(&app).len(), 1);

        app.shell_prefix_active = true;

        let lines = shell_status_lines(&app);
        assert_eq!(lines.len(), 2);

        let hint = lines[1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(hint.contains("d: detach"));
        assert!(hint.contains("o: others"));
        assert!(hint.contains("[: scroll"));
    }
}
