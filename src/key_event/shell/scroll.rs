// Scrollback of the attached shell view.
//
// The shell's history lives in its `vt100` screen: `set_scrollback(n)` moves
// the viewport `n` lines up, `0` being the live screen. Nothing is copied
// here; the parser owns the position, and this module only moves it and keeps
// the view mode (keys and wheel are owned by hire) in sync.
//
// Scrolling up enters the mode; scrolling back to the bottom leaves it, so the
// keys become the shell's again as soon as the live view is visible.

use crate::{app::App, utils::ShellId};

/// How many lines one wheel notch scrolls.
pub const WHEEL_LINES: usize = 3;

/// What a scroll binding does. Parsed from `keymap.toml` (`shell_scroll` /
/// `shell_scroll_mode`) and executed by [`apply`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollCommand {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Quit,
}

impl ScrollCommand {
    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        Ok(match value.trim() {
            "scroll up" => Self::Up,
            "scroll down" => Self::Down,
            "scroll page up" => Self::PageUp,
            "scroll page down" => Self::PageDown,
            "scroll top" => Self::Top,
            "scroll bottom" => Self::Bottom,
            "scroll quit" => Self::Quit,
            _ => anyhow::bail!("Unknown shell scroll command `{value}`"),
        })
    }
}

/// Enter the scroll mode at the live screen; the next keys scroll.
pub fn enter(app: &mut App, id: ShellId) {
    app.shell_prefix_active = false;
    set_offset(app, id, 0);
    app.shell_scroll_mode = true;
}

/// Return to the live screen and give the keys back to the shell.
pub fn leave(app: &mut App, id: ShellId) {
    set_offset(app, id, 0);
    app.shell_scroll_mode = false;
}

/// Run one scroll binding.
pub fn apply(app: &mut App, id: ShellId, command: ScrollCommand) {
    match command {
        ScrollCommand::Up => move_by(app, id, 1),
        ScrollCommand::Down => move_by(app, id, -1),
        ScrollCommand::PageUp => {
            let page = page_lines(app);
            move_by(app, id, page as isize)
        },
        ScrollCommand::PageDown => {
            let page = page_lines(app);
            move_by(app, id, -(page as isize))
        },
        ScrollCommand::Top => {
            set_offset(app, id, usize::MAX);
            set_mode(app, id);
        },
        ScrollCommand::Bottom | ScrollCommand::Quit => leave(app, id),
    }
}

/// One wheel notch; up enters the scroll mode, down leaves it at the bottom.
pub fn handle_wheel(app: &mut App, id: ShellId, up: bool) {
    let delta = WHEEL_LINES as isize;

    move_by(app, id, if up { delta } else { -delta });
}

/// A page keeps one line of the previous page visible.
fn page_lines(app: &App) -> usize {
    (app.shell_area.height as usize).saturating_sub(1).max(1)
}

/// Move the viewport by `delta` lines (negative scrolls towards the live
/// screen) and update the mode: any history above the view keeps the mode on,
/// the bottom leaves it.
fn move_by(app: &mut App, id: ShellId, delta: isize) {
    let Some(current) = offset(app, id) else {
        return
    };

    let target = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize)
    };

    set_offset(app, id, target);
    set_mode(app, id);
}

/// The mode follows the viewport: scrolled back means hire owns the keys.
fn set_mode(app: &mut App, id: ShellId) {
    app.shell_scroll_mode = offset(app, id).is_some_and(|offset| offset > 0);
}

fn offset(app: &App, id: ShellId) -> Option<usize> {
    app.shell_sessions
        .get(id)
        .map(|session| session.parser().screen().scrollback())
}

/// `vt100` clamps the offset to the available history, so callers may pass
/// `usize::MAX` for "the top".
fn set_offset(app: &mut App, id: ShellId, offset: usize) {
    if let Some(session) = app.shell_sessions.get(id) {
        session.parser().screen_mut().set_scrollback(offset);
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::*;
    use crate::key_event::shell::GraphicsSupport;

    fn open_test_shell(app: &mut App) -> ShellId {
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

        id
    }

    /// Shrink the screen so a few lines of output scroll off the top.
    fn small_screen(app: &mut App, id: ShellId) {
        let session = app.shell_sessions.get_mut(id).unwrap();
        session.resize(4, 20).unwrap();
    }

    fn feed(app: &App, id: ShellId, text: &str) {
        let session = app.shell_sessions.get(id).unwrap();
        session.parser().process(text.as_bytes());
    }

    fn scrolled_lines(app: &App, id: ShellId) -> usize {
        offset(app, id).unwrap()
    }

    #[test]
    fn test_apply_moves_one_line_and_leaves_at_the_bottom() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        small_screen(&mut app, id);
        app.shell_area.height = 4;
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n");

        apply(&mut app, id, ScrollCommand::Up);

        assert!(app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 1);

        apply(&mut app, id, ScrollCommand::Down);

        assert!(!app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 0);
    }

    #[test]
    fn test_page_and_top_and_bottom() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        small_screen(&mut app, id);
        app.shell_area.height = 4;
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n");

        apply(&mut app, id, ScrollCommand::PageUp);
        assert!(app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 3);

        apply(&mut app, id, ScrollCommand::Top);
        let top = scrolled_lines(&app, id);
        assert!(top >= 3);

        apply(&mut app, id, ScrollCommand::Bottom);

        assert!(!app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 0);
    }

    #[test]
    fn test_quit_leaves_the_mode() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        small_screen(&mut app, id);
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");

        enter(&mut app, id);
        apply(&mut app, id, ScrollCommand::Up);
        assert!(app.shell_scroll_mode);

        apply(&mut app, id, ScrollCommand::Quit);

        assert!(!app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 0);
    }

    #[test]
    fn test_wheel_enters_and_leaves() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        small_screen(&mut app, id);
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n");

        handle_wheel(&mut app, id, true);

        assert!(app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), WHEEL_LINES);

        handle_wheel(&mut app, id, false);

        assert!(!app.shell_scroll_mode);
        assert_eq!(scrolled_lines(&app, id), 0);
    }

    #[test]
    fn test_wheel_up_without_history_does_not_enter() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);

        handle_wheel(&mut app, id, true);

        assert!(!app.shell_scroll_mode);
    }
}
