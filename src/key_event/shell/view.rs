// Shell view.
//
// Opening, attaching, detaching and killing shell sessions, plus the key
// handling and the numbered selection used while a shell is attached.

use std::{
    io::{stdout, Write},
    time::{Duration, Instant},
};

use ratatui_image::picker::ProtocolType;

use ratatui::crossterm::{
    event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind},
    execute,
    style::Print,
};
use vt100::MouseProtocolMode;

use crate::{
    app::App,
    error::{AppResult, ErrorType, NotFoundType},
    key_event::TabId,
    rt_error,
    utils::{Block, ShellId},
};

use super::{
    button_bit,
    default_shell,
    encode_key,
    encode_mouse,
    encode_paste,
    graphics::GraphicsEvent,
    inside as mouse_inside,
    GraphicsSupport,
};

/// How long a partial shell number is kept before it is committed.
const DIGIT_COMMIT_DELAY: Duration = Duration::from_millis(450);

pub fn open_shell(app: &mut App) -> AppResult<()> {
    let program = default_shell(app)?;
    let tab_id = app.tab_list.current_tab().id;
    let cwd = app.path.to_owned();
    let graphics = graphics_support(app);

    let id = app.shell_sessions.open(tab_id, &program, &cwd, graphics)?;

    let tab = app.tab_list.current_tab_mut();
    tab.shells.push(id);
    tab.active_shell = Some(id);

    attach(app, id)
}

pub fn attach(app: &mut App, id: ShellId) -> AppResult<()> {
    let (tab_id, alive) = match app.shell_sessions.get(id) {
        Some(session) => (session.tab, session.alive),
        None => return Err(ErrorType::NotFound(NotFoundType::None).pack()),
    };

    if !alive {
        rt_error!("This shell has already exited")
    }

    if let Some(tab) = app.tab_list.tab_by_id_mut(tab_id) {
        tab.active_shell = Some(id);
    }

    app.switch_case = None;
    app.command_error = false;
    app.command_expand = false;
    app.command_scroll = None;
    app.command_completion.reset();
    app.shell_prefix_active = false;
    app.shell_pending_kill = None;
    app.shell_pending_kill_all = None;
    app.shell_notice = None;
    app.mouse_buttons = 0;
    app.clear_shell_digits();
    app.selected_block = Block::Shell(id);
    app.clear_screen = true;

    Ok(())
}

pub fn detach(app: &mut App) {
    let in_root = app.root();

    app.shell_prefix_active = false;
    app.shell_pending_kill = None;
    app.shell_pending_kill_all = None;
    app.shell_notice = None;
    app.mouse_buttons = 0;
    app.clear_shell_digits();
    app.selected_block = Block::Browser(in_root);
    app.clear_screen = true;

    // The shell may have created, renamed or deleted files while it was in
    // front, so the browser's lists are stale.
    refresh_after_shell(app);
}

/// Re-read the current directory after leaving the shell view.
///
/// The selected file is selected again when it still exists. Errors are
/// reported through the usual error banner instead of being dropped: the
/// directory may have been removed from inside the shell.
fn refresh_after_shell(app: &mut App) {
    let selected = app.get_file_saver().map(|file| file.name.to_owned());
    // Re-read the directory the browser shows, not `current_path()`: at the
    // root directory the latter is the selected parent entry, and refreshing
    // must not navigate into it.
    let dir = app.path.to_owned();

    if let Err(err) = app.goto_dir(dir, Some(app.hide_files)) {
        app.app_error.append_errors(err.iter());
        return
    }

    let Some(name) = selected else {
        return
    };

    if let Err(err) = app.file_search_sync(name, true) {
        app.app_error.append_errors(err.iter());
    }
}

/// Attach the next/previous live shell of the same tab.
pub fn next_shell(app: &mut App, forward: bool) -> AppResult<()> {
    let Some(current) = app.selected_block.shell_id() else {
        return Ok(())
    };
    let Some(tab_id) = app.shell_sessions.get(current).map(|s| s.tab) else {
        return Ok(())
    };

    let shells = app
        .tab_list
        .tab_by_id(tab_id)
        .map(|tab| tab.shells.clone())
        .unwrap_or_default();

    let alive = shells
        .into_iter()
        .filter(|id| app.shell_sessions.is_alive(*id))
        .collect::<Vec<_>>();

    if alive.len() <= 1 {
        return Ok(())
    }

    let pos = alive.iter().position(|id| *id == current).unwrap_or(0);
    let next = if forward {
        (pos + 1) % alive.len()
    } else {
        (pos + alive.len() - 1) % alive.len()
    };

    attach(app, alive[next])
}

/// Handle one key while a shell view owns the input.
pub fn handle_view_key(key: KeyEvent, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    // A kill (or kill-all) confirmation is waiting for `y`.
    let pending_all = app.shell_pending_kill_all.take();
    let pending_one = app.shell_pending_kill.take();

    if pending_all.is_some() || pending_one.is_some() {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            match (pending_all, pending_one) {
                (Some(tab_id), _) => {
                    app.close_tab_shells(tab_id);
                },
                (None, Some(pending)) => close_shell(app, pending),
                (None, None) => (),
            }
        }

        return Ok(())
    }

    // Digits typed after the prefix select a shell by number.
    if !app.shell_digits.is_empty() {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                push_digit(app, c);
                return Ok(())
            },
            KeyCode::Enter => {
                app.commit_shell_digits();
                return Ok(())
            },
            KeyCode::Esc => {
                app.clear_shell_digits();
                return Ok(())
            },
            // Anything else cancels the number and is still forwarded.
            _ => app.clear_shell_digits(),
        }
    }

    if app.shell_prefix_active {
        app.shell_prefix_active = false;

        // Pressing the prefix twice sends a literal prefix byte.
        if app.shell_prefix.matches(&key) {
            return write_key(app, id, &key)
        }

        match key.code {
            KeyCode::Char('d') => detach(app),
            KeyCode::Char('n') => return next_shell(app, true),
            KeyCode::Char('p') => return next_shell(app, false),
            KeyCode::Char('c') => return open_shell(app),
            KeyCode::Char('x') => app.shell_pending_kill = Some(id),
            // Kill every shell mounted on this tab; the tab stays open.
            KeyCode::Char('D') => {
                if let Some(tab_id) = app.shell_sessions.get(id).map(|session| session.tab) {
                    app.shell_pending_kill_all = Some(tab_id);
                }
            },
            KeyCode::Char(c) if c.is_ascii_digit() => push_digit(app, c),
            KeyCode::Esc => (),
            _ => return write_key(app, id, &key),
        }

        return Ok(())
    }

    if app.shell_prefix.matches(&key) {
        app.shell_prefix_active = true;
        return Ok(())
    }

    write_key(app, id, &key)
}

/// Forward a mouse event to the attached shell.
pub fn handle_view_mouse(mouse: MouseEvent, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    // The kill confirmation owns the input; do not leak clicks to the child.
    if app.shell_pending_kill.is_some() || app.shell_pending_kill_all.is_some() {
        return Ok(())
    }

    app.shell_prefix_active = false;
    app.clear_shell_digits();

    let area = app.shell_area;
    let inside_area = mouse_inside(area, mouse.column, mouse.row);

    match mouse.kind {
        MouseEventKind::Down(button) => {
            if !inside_area {
                return Ok(())
            }
            app.mouse_buttons |= button_bit(button);
        },

        MouseEventKind::Up(button) => {
            // A release which started inside the shell still has to be
            // delivered, even if the pointer is now over the title bar.
            if !inside_area && app.mouse_buttons == 0 {
                return Ok(())
            }
            app.mouse_buttons &= !button_bit(button);
        },

        MouseEventKind::Drag(_) => {
            if app.mouse_buttons == 0 {
                return Ok(())
            }
        },

        MouseEventKind::Moved => {
            if !inside_area {
                return Ok(())
            }
        },

        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown |
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            if !inside_area {
                return Ok(())
            }
        },
    }

    let (mode, encoding) = match app.shell_sessions.get(id) {
        Some(session) => {
            let parser = session.parser();
            let screen = parser.screen();
            (screen.mouse_protocol_mode(), screen.mouse_protocol_encoding())
        },
        None => return Ok(()),
    };

    let Some(bytes) = encode_mouse(&mouse, mode, encoding, area) else {
        return Ok(())
    };

    if let Some(session) = app.shell_sessions.get_mut(id) {
        let _ = session.write(&bytes);
    }

    Ok(())
}

/// Forward a bracketed paste (or typed text) to the attached shell.
pub fn handle_view_paste(text: String, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    if app.shell_pending_kill.is_some() || app.shell_pending_kill_all.is_some() {
        return Ok(())
    }

    app.shell_prefix_active = false;
    app.clear_shell_digits();

    let bracketed = app
        .shell_sessions
        .get(id)
        .map(|session| session.parser().screen().bracketed_paste())
        .unwrap_or(false);
    let bytes = encode_paste(&text, bracketed);

    if let Some(session) = app.shell_sessions.get_mut(id) {
        let _ = session.write(&bytes);
    }

    Ok(())
}

impl App<'_> {
    /// Reap exited sessions and repair the tab/view state.
    pub fn handle_shell_events(&mut self) {
        let exited = self.shell_sessions.drain_exits();

        for id in exited {
            let tab_id = self.shell_sessions.get(id).map(|session| session.tab);
            let was_attached = self.selected_block.shell_id() == Some(id);

            self.shell_sessions.close(id);

            let next = tab_id
                .and_then(|tab_id| self.remove_shell_from_tab(tab_id, id));

            if was_attached {
                match next {
                    Some(next) => {
                        if attach(self, next).is_err() {
                            detach(self);
                        }
                    },
                    None => detach(self),
                }
            }
        }
    }

    /// Drop `id` from its tab and return the shell which should be attached
    /// next when the removed one was the active shell of the tab.
    pub fn remove_shell_from_tab(
        &mut self,
        tab_id: TabId,
        id: ShellId,
    ) -> Option<ShellId>
    {
        let shells = self
            .tab_list
            .tab_by_id(tab_id)
            .map(|tab| tab.shells.clone())
            .unwrap_or_default();

        let surviving = shells
            .into_iter()
            .filter(|shell| *shell != id)
            .collect::<Vec<_>>();

        // Picking the next shell must happen before `tab_list` is borrowed
        // mutably.
        let next = surviving
            .iter()
            .rev()
            .copied()
            .find(|shell| self.shell_sessions.is_alive(*shell));

        let mut was_active = false;
        if let Some(tab) = self.tab_list.tab_by_id_mut(tab_id) {
            was_active = tab.active_shell == Some(id);
            tab.shells = surviving;

            if was_active {
                tab.active_shell = next;
            }
        }

        if was_active {
            next
        } else {
            None
        }
    }

    /// Kill every shell mounted on `tab_id` and forget them on the tab.
    ///
    /// The tab itself stays open. The attached shell is detached first when it
    /// belongs to the tab, so the view never points at a session which is
    /// being dropped. Returns how many shells were closed.
    ///
    /// This is the single place which clears a tab's shells: the tab menu's
    /// `D`, the shell view's `C-s D`, closing a tab and replacing the tab list
    /// (stored tabs) all go through it.
    pub fn close_tab_shells(&mut self, tab_id: TabId) -> usize {
        let shells = self
            .tab_list
            .tab_by_id(tab_id)
            .map(|tab| tab.shells.clone())
            .unwrap_or_default();

        if shells.is_empty() {
            return 0
        }

        if let Some(attached) = self.selected_block.shell_id() {
            // Trust the session's owner as well as the tab's list: the list is
            // what is being cleared, and the view must never survive pointing
            // at a session which is about to be dropped.
            let owns_attached = self
                .shell_sessions
                .get(attached)
                .map(|session| session.tab == tab_id)
                .unwrap_or(false);

            if owns_attached || shells.contains(&attached) {
                detach(self);
            }
        }

        self.shell_sessions.close_many(&shells);

        if let Some(tab) = self.tab_list.tab_by_id_mut(tab_id) {
            tab.shells.clear();
            tab.active_shell = None;
        }

        shells.len()
    }

    pub fn clear_shell_digits(&mut self) {
        self.shell_digits.clear();
        self.shell_digit_deadline = None;
    }

    /// Commit typed digits once their deadline passed.
    pub fn tick_shell_digits(&mut self) {
        if self
            .shell_digit_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.commit_shell_digits();
        }
    }

    /// Attach the shell selected by the digits typed after the prefix.
    pub fn commit_shell_digits(&mut self) {
        if self.shell_digits.is_empty() {
            return
        }

        let digits = std::mem::take(&mut self.shell_digits);
        self.shell_digit_deadline = None;

        let Ok(idx) = digits.parse::<usize>() else {
            return
        };

        let Some(id) = self
            .tab_list
            .current_tab()
            .shells
            .get(idx)
            .copied()
        else {
            return
        };

        if let Err(err) = attach(self, id) {
            self.app_error.append_errors(err.iter());
        }
    }

    /// Keep the outer terminal's mouse and bracketed-paste modes in sync with
    /// the attached shell. Called once per main-loop iteration: the child may
    /// enable or disable them at any time.
    pub fn sync_terminal_modes(&mut self) {
        let (mouse, paste) = match self
            .selected_block
            .shell_id()
            .and_then(|id| self.shell_sessions.get(id))
        {
            Some(session) => {
                let parser = session.parser();
                let screen = parser.screen();
                (screen.mouse_protocol_mode(), screen.bracketed_paste())
            },
            None => (MouseProtocolMode::None, false),
        };

        if mouse != self.outer_mouse_mode {
            let _ = set_outer_mouse_mode(mouse);
            self.outer_mouse_mode = mouse;
            // A mode switch cancels any half-tracked drag.
            self.mouse_buttons = 0;
        }

        if paste != self.outer_bracketed_paste {
            let _ = set_outer_bracketed_paste(paste);
            self.outer_bracketed_paste = paste;
        }
    }

    /// Restore the outer terminal before leaving hire.
    pub fn disable_terminal_modes(&mut self) {
        if self.outer_mouse_mode != MouseProtocolMode::None {
            let _ = set_outer_mouse_mode(MouseProtocolMode::None);
            self.outer_mouse_mode = MouseProtocolMode::None;
        }

        if self.outer_bracketed_paste {
            let _ = set_outer_bracketed_paste(false);
            self.outer_bracketed_paste = false;
        }

        self.mouse_buttons = 0;
    }
}

fn write_key(app: &mut App, id: ShellId, key: &KeyEvent) -> AppResult<()> {
    // Arrow keys (and Home/End) are sent as `ESC O A` while the child has
    // enabled the application cursor mode, and as `ESC [ A` otherwise. Curses
    // programs such as vim and less enable it and may ignore the other form.
    let application_cursor = app
        .shell_sessions
        .get(id)
        .map(|session| session.parser().screen().application_cursor())
        .unwrap_or(false);

    let Some(bytes) = encode_key(key, application_cursor) else {
        return Ok(())
    };

    if let Some(session) = app.shell_sessions.get_mut(id) {
        // A write failure means the child is gone; its exit event will clean
        // the view up, so it is not reported as an application error.
        let _ = session.write(&bytes);
    }

    Ok(())
}

fn close_shell(app: &mut App, id: ShellId) {
    let tab_id = app.shell_sessions.get(id).map(|session| session.tab);
    let was_attached = app.selected_block.shell_id() == Some(id);

    app.shell_sessions.close(id);

    let next = tab_id.and_then(|tab_id| app.remove_shell_from_tab(tab_id, id));

    if was_attached {
        match next {
            Some(next) => {
                if attach(app, next).is_err() {
                    detach(app);
                }
            },
            None => detach(app),
        }
    }
}

/// Append one digit to the pending shell number and commit it when it can no
/// longer grow into a valid index.
fn push_digit(app: &mut App, digit: char) {
    app.shell_digits.push(digit);
    app.shell_digit_deadline = Some(Instant::now() + DIGIT_COMMIT_DELAY);

    let max = app
        .tab_list
        .current_tab()
        .shells
        .len()
        .saturating_sub(1);
    let width = max.to_string().len().max(1);

    let value = match app.shell_digits.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            app.clear_shell_digits();
            return
        },
    };

    // Out of range: forget the number instead of selecting the wrong shell.
    if value > max {
        app.clear_shell_digits();
        return
    }

    // Enough digits to cover the whole range, or no further digit could still
    // produce a valid index.
    if app.shell_digits.len() >= width || value.saturating_mul(10) > max {
        app.commit_shell_digits();
    }
}

/// Enable the outer terminal's mouse tracking in the mode the child needs.
///
/// SGR encoding (`?1006`) is always requested so crossterm reports releases
/// and large coordinates reliably; the event is re-encoded for the child.
fn set_outer_mouse_mode(mode: MouseProtocolMode) -> std::io::Result<()> {
    let mut sequence = String::from(
        "\x1b[?9l\x1b[?1000l\x1b[?1001l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l",
    );

    match mode {
        MouseProtocolMode::None => (),
        MouseProtocolMode::Press | MouseProtocolMode::PressRelease => {
            sequence.push_str("\x1b[?1000h")
        },
        MouseProtocolMode::ButtonMotion => sequence.push_str("\x1b[?1002h"),
        MouseProtocolMode::AnyMotion => sequence.push_str("\x1b[?1003h"),
    }

    if mode != MouseProtocolMode::None {
        sequence.push_str("\x1b[?1006h");
    }

    execute!(stdout(), Print(sequence))
}

fn set_outer_bracketed_paste(enabled: bool) -> std::io::Result<()> {
    execute!(
        stdout(),
        Print(if enabled {
            "\x1b[?2004h"
        } else {
            "\x1b[?2004l"
        })
    )
}

/// The image support a new shell session is allowed to offer.
///
/// Kitty graphics are forwarded to the real terminal, so they are only offered
/// when the real terminal speaks kitty. Inside tmux the sequences would have to
/// be wrapped for tmux passthrough, which is not done yet, so nothing is
/// offered there.
fn graphics_support(app: &App) -> GraphicsSupport {
    if in_tmux() {
        return GraphicsSupport::disabled()
    }

    match app.image_preview.protocol_type() {
        Some(ProtocolType::Kitty) => GraphicsSupport::kitty(
            app.image_preview.font_size().unwrap_or((10, 20)),
        ),
        _ => GraphicsSupport::disabled(),
    }
}

fn in_tmux() -> bool {
    std::env::var("TERM").is_ok_and(|term| term.starts_with("tmux")) ||
        std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "tmux")
}

impl App<'_> {
    /// Forward the graphics the shells produced to the real terminal and
    /// answer the capability queries they sent while starting up.
    ///
    /// A forwarded image is followed by a full repaint: the placeholder cells
    /// the child painted must be written *after* the terminal knows the image,
    /// otherwise they refer to an image it has never seen.
    pub fn handle_shell_graphics(&mut self) -> AppResult<()> {
        let mut forwarded = false;

        for (id, event) in self.shell_sessions.drain_graphics() {
            match event {
                GraphicsEvent::Reply(bytes) => {
                    if let Some(session) = self.shell_sessions.get_mut(id) {
                        let _ = session.write(&bytes);
                    }
                },

                GraphicsEvent::Forward(bytes) => {
                    let mut stdout = stdout();
                    stdout.write_all(&bytes)?;
                    stdout.flush()?;
                    forwarded = true;
                },
            }
        }

        if forwarded {
            self.clear_screen = true;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ratatui::crossterm::event::KeyModifiers;

    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-shell-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        handle_view_key(KeyEvent::new(code, modifiers), app).unwrap();
    }

    /// Mount a `/bin/sh` session on the current tab, without attaching it.
    fn open_test_shell(app: &mut App) -> ShellId {
        let tab_id = app.tab_list.current_tab().id;
        let id = app
            .shell_sessions
            .open(tab_id, "/bin/sh", Path::new("/"), GraphicsSupport::disabled())
            .unwrap();

        let tab = app.tab_list.current_tab_mut();
        tab.shells.push(id);
        tab.active_shell = Some(id);

        id
    }

    /// `C-s D` asks before killing every shell of the tab; `y` then kills them
    /// all and leaves the browser behind.
    #[test]
    fn test_prefix_d_kills_all_shells_after_confirmation() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        open_test_shell(&mut app);
        let last = open_test_shell(&mut app);
        attach(&mut app, last).unwrap();

        assert_eq!(app.tab_list.current_tab().shells.len(), 2);

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('D'), KeyModifiers::SHIFT);

        assert!(app.shell_pending_kill_all.is_some());

        // Nothing is killed before the confirmation.
        assert_eq!(app.tab_list.current_tab().shells.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 2);

        press(&mut app, KeyCode::Char('y'), KeyModifiers::empty());

        assert!(app.shell_pending_kill_all.is_none());
        assert!(!app.selected_block.is_shell());
        assert!(app.tab_list.current_tab().shells.is_empty());
        assert_eq!(app.shell_sessions.alive_count(), 0);
    }

    /// Any other key cancels the kill-all confirmation.
    #[test]
    fn test_prefix_d_cancel_keeps_the_shells() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('D'), KeyModifiers::SHIFT);
        press(&mut app, KeyCode::Char('n'), KeyModifiers::empty());

        assert!(app.shell_pending_kill_all.is_none());
        assert!(app.selected_block.is_shell());
        assert_eq!(app.tab_list.current_tab().shells.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 1);
    }

    /// Leaving the shell view re-reads the directory, so files the shell
    /// created appear at once, and the selected file stays selected.
    #[test]
    fn test_detach_refreshes_the_browser() {
        let dir = temp_dir("detach-refresh");
        fs::write(dir.join("a.txt"), "").unwrap();
        fs::write(dir.join("b.txt"), "").unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();
        app.file_search_sync(String::from("b.txt"), true).unwrap();

        assert_eq!(
            app.get_file_saver().map(|file| file.name.as_str()),
            Some("b.txt"),
        );

        // The shell creates a file while it is in front.
        fs::write(dir.join("c.txt"), "").unwrap();
        app.selected_block = Block::Shell(ShellId(0));

        detach(&mut app);

        assert!(matches!(app.selected_block, Block::Browser(_)));
        assert!(app.current_files.iter().any(|file| file.name == "c.txt"));
        assert_eq!(
            app.get_file_saver().map(|file| file.name.as_str()),
            Some("b.txt"),
            "the selected file is kept across the refresh",
        );

        // A file deleted in the shell disappears as well.
        fs::remove_file(dir.join("a.txt")).unwrap();
        app.selected_block = Block::Shell(ShellId(0));

        detach(&mut app);

        assert!(!app.current_files.iter().any(|file| file.name == "a.txt"));

        fs::remove_dir_all(&dir).unwrap();
    }
}
