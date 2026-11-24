//! Opens, attaches, detaches and kills shell sessions, and handles the keys
//! and numbered selection of the attached shell.

use std::{
    io::{stdout, Write},
    time::{Duration, Instant},
};

use ratatui_image::picker::ProtocolType;

use ratatui::crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    style::Print,
};
use vt100::MouseProtocolMode;

use crate::{
    app::App,
    error::{AppResult, ErrorType, NotFoundType},
    key_event::{tab::switch_to_tab, TabId},
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
    scroll,
    shell_operation,
    ClosedShell,
    GraphicsSupport,
    ShellOrigin,
};

/// How long a partial shell number is kept before it is committed.
const DIGIT_COMMIT_DELAY: Duration = Duration::from_millis(450);

/// Open a new shell on the current tab and attach it.
///
/// `origin` is the view the opening key belonged to; it decides where the view
/// goes when the shell goes away (see [`leave_closed_shell`]).
pub fn open_shell(app: &mut App, origin: ShellOrigin) -> AppResult<()> {
    let program = default_shell(app)?;
    let tab_id = app.tab_list.current_tab().id;
    let cwd = app.path.to_owned();
    let graphics = graphics_support(app);

    let id = app
        .shell_sessions
        .open(tab_id, &program, &cwd, graphics, origin)?;

    let tab = app.tab_list.current_tab_mut();
    tab.shells.push(id);
    tab.active_shell = Some(id);

    attach(app, id)
}

/// Attach the shell which the current tab used last.
///
/// `Tab::active_shell` follows every attach, so going back to the shell which
/// was in front is one key instead of the shell menu. A tab with no shell at
/// all borrows the shell attached most recently on another tab, so `B` still
/// reaches background shells; `attach` switches to that tab for the view and
/// `detach` returns to this one.
pub fn attach_active(app: &mut App) -> AppResult<()> {
    let (tab_id, active, has_shells) = {
        let tab = app.tab_list.current_tab();
        (tab.id, tab.active_shell, !tab.shells.is_empty())
    };

    if has_shells {
        let Some(id) = active else {
            rt_error!("No shell is active on the current tab")
        };

        return attach(app, id)
    }

    let Some(id) = recent_shell(app, tab_id) else {
        rt_error!("No shell is mounted on this tab or another tab")
    };

    attach(app, id)
}

/// Attach the shell pinned with `C-s *`.
///
/// The pin survives detaching, so the program kept in the background is one
/// command away from the browser and from any attached shell (`C-s b`). A pin
/// to an exited session is dropped and reported, not left silently dead.
pub fn attach_pinned(app: &mut App) -> AppResult<()> {
    let Some(id) = app.pinned_shell else {
        rt_error!("No shell is pinned (pin one with `C-s *`)")
    };

    if !app.shell_sessions.is_alive(id) {
        app.pinned_shell = None;
        rt_error!("The pinned shell has exited")
    }

    attach(app, id)
}

pub fn toggle_pin(app: &mut App, id: ShellId) {
    app.pinned_shell = if app.pinned_shell == Some(id) {
        None
    } else {
        Some(id)
    };
}

fn recent_shell(app: &App, current_tab: TabId) -> Option<ShellId> {
    app.shell_recent.iter().copied().find(|id| {
        app.shell_sessions.get(*id).is_some_and(|session| {
            session.alive && session.tab != current_tab
        })
    })
}

/// Attach `id` and make it the view's shell.
///
/// An attached shell must belong to the current tab: the status line, the
/// prefix commands and the exit cleanup all read `current_tab()`. Attaching
/// another tab's shell switches to that tab for the duration, and
/// `shell_return_tab` remembers where `detach` goes back to.
pub fn attach(app: &mut App, id: ShellId) -> AppResult<()> {
    let (tab_id, alive) = match app.shell_sessions.get(id) {
        Some(session) => (session.tab, session.alive),
        None => return Err(ErrorType::NotFound(NotFoundType::None).pack()),
    };

    if !alive {
        rt_error!("This shell has already exited")
    }

    let current_tab = app.tab_list.current_tab().id;

    if current_tab != tab_id {
        switch_to_tab(app, tab_id)?;
        app.shell_return_tab.get_or_insert(current_tab);
    }

    if let Some(tab) = app.tab_list.tab_by_id_mut(tab_id) {
        tab.active_shell = Some(id);
    }

    // The menu, the question and the expanded command line belonged to the
    // view which is being left.
    app.switch_case = None;
    app.shell_menu_origin = None;
    app.command_error = false;
    app.command_expand = false;
    app.command_scroll = None;
    app.command_completion.reset();
    app.shell_prefix_active = false;
    app.shell_scroll_mode = false;
    app.shell_pending_kill = None;
    app.shell_pending_kill_others = None;
    app.shell_pending_kill_all = None;
    app.shell_pending_browser = false;
    app.shell_notice = None;
    app.mouse_buttons = 0;
    app.clear_shell_digits();
    app.selected_block = Block::Shell(id);
    app.clear_screen = true;

    // A shell is always attached at the live screen.
    if let Some(session) = app.shell_sessions.get(id) {
        session.parser().screen_mut().set_scrollback(0);
    }

    app.note_attached_shell(id);

    Ok(())
}

pub fn detach(app: &mut App) {
    let in_root = app.root();

    // Every running shell's life now includes the browser, not only the one in
    // front: whichever closes later asks before entering it again (see
    // `leave_closed_shell`). A shell opened after this detach is not marked.
    app.shell_sessions.mark_returned_to_browser();

    app.shell_prefix_active = false;
    app.shell_scroll_mode = false;
    app.shell_pending_kill = None;
    app.shell_pending_kill_others = None;
    app.shell_pending_kill_all = None;
    app.shell_pending_browser = false;
    app.shell_notice = None;
    app.mouse_buttons = 0;
    app.clear_shell_digits();
    app.selected_block = Block::Browser(in_root);
    app.clear_screen = true;

    // The shell may have created, renamed or deleted files while it was in
    // front, so the browser's lists are stale.
    refresh_after_shell(app);

    // A borrowed shell returns the user to the tab they came from. The shell's
    // tab was just refreshed and `switch_to_tab` saves that state, so the next
    // visit shows the files the shell changed.
    if let Some(origin) = app.shell_return_tab.take()
        && origin != app.tab_list.current_tab().id
    {
        match switch_to_tab(app, origin) {
            // `selected_block` was built from the shell's tab; the browser
            // flag follows the tab which is current now.
            Ok(()) => app.selected_block = Block::Browser(app.root()),
            Err(err) => app.app_error.append_errors(err.iter()),
        }
    }
}

/// Re-read the current directory after leaving the shell view.
///
/// The selected file is selected again when it still exists. Errors go to the
/// error banner: the directory may have been removed from inside the shell.
fn refresh_after_shell(app: &mut App) {
    let selected = app.get_file_saver().map(|file| file.name.to_owned());
    // Re-read `app.path`, not `current_path()`: at the root directory the
    // latter is the selected parent entry, which must not be navigated into.
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

pub fn handle_view_key(key: KeyEvent, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    // Any key answers a pending kill confirmation; `take` clears it either
    // way, so only `y` performs the kill.
    let pending_all = app.shell_pending_kill_all.take();
    let pending_others = app.shell_pending_kill_others.take();
    let pending_one = app.shell_pending_kill.take();

    if pending_all.is_some() || pending_others.is_some() || pending_one.is_some()
    {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            if let Some(tab_id) = pending_all {
                app.close_tab_shells(tab_id);
            } else if let Some(keep) = pending_others {
                close_other_shells(app, keep);
            } else if let Some(pending) = pending_one {
                close_shell(app, pending);
            }
        }

        return Ok(())
    }

    // A closed shell offers the browser: `y` takes it, any other key stays on
    // the shell the view fell back to.
    if app.shell_pending_browser {
        app.shell_pending_browser = false;

        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            detach(app);
        }

        return Ok(())
    }

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

    // In scroll mode hire owns the keyboard: configured scroll bindings run,
    // everything else is swallowed (never forwarded to the child). The prefix
    // chord is the exception: it leaves the mode and arms the usual prefix
    // command, so `C-s d`, `C-s x`, … still work after scrolling.
    if app.shell_scroll_mode {
        if app.shell_prefix.matches(&key) {
            scroll::leave(app, id);
            app.shell_prefix_active = true;
            return Ok(())
        }

        let command = app
            .keymap
            .shell_scroll_mode_get(&key)
            .or_else(|| app.keymap.shell_scroll_get(&key));

        if let Some(command) = command {
            scroll::apply(app, id, command);
        }

        return Ok(())
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
            KeyCode::Char('c') => return open_shell(app, ShellOrigin::Shell),
            KeyCode::Char('x') => app.shell_pending_kill = Some(id),
            // Kill every shell mounted on this tab; the tab stays open.
            KeyCode::Char('D') => {
                if let Some(tab_id) = app.shell_sessions.get(id).map(|session| session.tab) {
                    app.shell_pending_kill_all = Some(tab_id);
                }
            },
            // Kill every other shell of the tab and keep this one.
            KeyCode::Char('o') => {
                if attached_tab_has_others(app, id) {
                    app.shell_pending_kill_others = Some(id);
                }
            },
            // The shell menu: the same page the browser's `T` opens. Closing
            // it without attaching anything comes back to this shell.
            KeyCode::Char('t') => return shell_operation(app),
            // Pin/unpin the attached shell; `C-s b` attaches it from anywhere.
            KeyCode::Char('*') => toggle_pin(app, id),
            KeyCode::Char('b') => return attach_pinned(app),
            // Browse the shell's history: the `shell_scroll_mode` keys then
            // scroll, and `q`/`Esc` returns to the live screen.
            KeyCode::Char('[') => scroll::enter(app, id),
            KeyCode::Char(c) if c.is_ascii_digit() => push_digit(app, c),
            KeyCode::Esc => (),
            _ => return write_key(app, id, &key),
        }

        return Ok(())
    }

    // Scrolling without entering the mode first (default `M-u`/`M-e`). A
    // matching key is consumed instead of being sent to the shell.
    if let Some(command) = app.keymap.shell_scroll_get(&key) {
        scroll::apply(app, id, command);
        return Ok(())
    }

    if app.shell_prefix.matches(&key) {
        app.shell_prefix_active = true;
        return Ok(())
    }

    write_key(app, id, &key)
}

pub fn handle_view_mouse(mouse: MouseEvent, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    // A pending confirmation owns the input; do not leak clicks to the child.
    if app.shell_pending_kill.is_some() ||
        app.shell_pending_kill_others.is_some() ||
        app.shell_pending_kill_all.is_some() ||
        app.shell_pending_browser
    {
        return Ok(())
    }

    app.shell_prefix_active = false;
    app.clear_shell_digits();

    let area = app.shell_area;
    let inside_area = mouse_inside(area, mouse.column, mouse.row);

    // The wheel belongs to the child (`vim`, `less`, …), so its own mouse
    // scrolling keeps working. `Shift` + wheel is hire's instead: it enters
    // the scroll mode (or scrolls it) and swallows the event.
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        if !inside_area {
            return Ok(())
        }

        let shift = mouse.modifiers.contains(KeyModifiers::SHIFT);

        if app.shell_scroll_mode || shift {
            scroll::handle_wheel(
                app,
                id,
                matches!(mouse.kind, MouseEventKind::ScrollUp),
            );

            return Ok(())
        }

        // Fall through: `encode_mouse` forwards it to the child when it asked
        // for mouse reporting. Without that reporting hire never receives the
        // event, so the terminal handles it (native scrolling/selection).
    }

    // Scroll mode owns the mouse too: a click must not reach the child while
    // the user is reading the history.
    if app.shell_scroll_mode {
        return Ok(())
    }

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

        // A wheel which got here was not claimed above, so the child gets it.
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

pub fn handle_view_paste(text: String, app: &mut App) -> AppResult<()> {
    let Some(id) = app.selected_block.shell_id() else {
        return Ok(())
    };

    if app.shell_pending_kill.is_some() ||
        app.shell_pending_kill_others.is_some() ||
        app.shell_pending_kill_all.is_some() ||
        app.shell_pending_browser ||
        app.shell_scroll_mode
    {
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
    /// Remember `id` as the most recently attached shell.
    ///
    /// The list is capped: it only serves `attach_active`'s fallback, which
    /// picks the newest entry whose session is still alive.
    fn note_attached_shell(&mut self, id: ShellId) {
        self.shell_recent.retain(|shell| *shell != id);
        self.shell_recent.insert(0, id);
        self.shell_recent.truncate(64);
    }

    /// Drop the state which points at `id`, once the session is gone.
    ///
    /// Called from every place which removes a shell from its tab, so a pin
    /// or the shell menu's origin never keeps pointing at a dead session.
    pub fn forget_shell(&mut self, id: ShellId) {
        if self.pinned_shell == Some(id) {
            self.pinned_shell = None;
        }

        if self.shell_menu_origin == Some(id) {
            self.shell_menu_origin = None;
        }
    }

    /// Close the shell menu and return to the view it was opened from.
    ///
    /// The menu replaces that view, so closing it without attaching anything
    /// attaches the shell again (`C-s t`) instead of dropping into the browser;
    /// a menu opened from the browser has no origin and leaves it in place.
    /// `quit_command_mode` calls this, so every close path goes through it.
    pub fn leave_shell_menu(&mut self) -> bool {
        let Some(origin) = self.shell_menu_origin.take() else {
            return false
        };

        if !self.shell_sessions.is_alive(origin) {
            return false
        }

        attach(self, origin).is_ok()
    }

    /// Reap exited sessions and repair the tab/view state.
    pub fn handle_shell_events(&mut self) {
        let exited = self.shell_sessions.drain_exits();

        for id in exited {
            let tab_id = self.shell_sessions.get(id).map(|session| session.tab);
            let was_attached = self.selected_block.shell_id() == Some(id);
            let closed = self
                .shell_sessions
                .get(id)
                .map(|session| session.closed_state())
                .unwrap_or_default();

            self.shell_sessions.close(id);

            let next = tab_id
                .and_then(|tab_id| self.remove_shell_from_tab(tab_id, id));

            if was_attached {
                leave_closed_shell(self, closed, next);
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

        self.forget_shell(id);

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
    /// The tab stays open. The attached shell is detached first when it
    /// belongs to the tab, so the view never points at a session being
    /// dropped. Every clear of a tab's shells goes through this function: the
    /// menu's `D`, the view's `C-s D`, closing a tab and stored tabs. Returns
    /// how many shells were closed.
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

        for id in &shells {
            self.forget_shell(*id);
        }

        if let Some(tab) = self.tab_list.tab_by_id_mut(tab_id) {
            tab.shells.clear();
            tab.active_shell = None;
        }

        shells.len()
    }

    /// Kill every shell mounted on `tab_id` except `keep`.
    ///
    /// `C-s o` uses it: the shell which owns the view survives and stays
    /// attached, and the tab keeps it as its active shell. Returns how many
    /// shells were closed.
    pub fn close_other_shells(&mut self, tab_id: TabId, keep: ShellId) -> usize {
        let shells = self
            .tab_list
            .tab_by_id(tab_id)
            .map(|tab| tab.shells.clone())
            .unwrap_or_default();

        let others = shells
            .into_iter()
            .filter(|id| *id != keep)
            .collect::<Vec<_>>();

        if others.is_empty() {
            return 0
        }

        // The view is supposed to point at `keep`, but never keep it pointing
        // at a session which is about to be dropped.
        if let Some(attached) = self.selected_block.shell_id()
            && others.contains(&attached)
        {
            detach(self);
        }

        self.shell_sessions.close_many(&others);

        for id in &others {
            self.forget_shell(*id);
        }

        if let Some(tab) = self.tab_list.tab_by_id_mut(tab_id) {
            tab.shells.retain(|id| *id == keep);
            tab.active_shell = Some(keep);
        }

        others.len()
    }

    pub fn clear_shell_digits(&mut self) {
        self.shell_digits.clear();
        self.shell_digit_deadline = None;
    }

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
        let (child_mouse, paste) = match self
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

        let mouse = outer_mouse_mode(child_mouse, self.shell_scroll_mode);

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
    let closed = app
        .shell_sessions
        .get(id)
        .map(|session| session.closed_state())
        .unwrap_or_default();

    app.shell_sessions.close(id);

    let next = tab_id.and_then(|tab_id| app.remove_shell_from_tab(tab_id, id));

    if was_attached {
        leave_closed_shell(app, closed, next);
    }
}

/// Move the view on after the attached shell was closed.
///
/// `next` is the tab's surviving shell, if any; a tab with a single shell goes
/// back to the browser. Otherwise the closed shell decides: one opened from
/// the browser and never left for it returns there, one opened from a shell
/// attaches the tab's previous shell, and one the user had already left for
/// the browser attaches that shell and asks (`y`) where to continue.
fn leave_closed_shell(
    app: &mut App,
    closed: ClosedShell,
    next: Option<ShellId>,
) {
    let Some(next) = next else {
        detach(app);
        return
    };

    if closed.origin == ShellOrigin::Browser && !closed.returned_to_browser {
        detach(app);
        return
    }

    if attach(app, next).is_err() {
        detach(app);
        return
    }

    // `attach` clears every pending confirmation, so the question is set after
    // it. The fallback shell is in front while the user answers.
    if closed.returned_to_browser {
        app.shell_pending_browser = true;
    }
}

fn attached_tab_has_others(app: &App, id: ShellId) -> bool {
    let Some(tab_id) = app.shell_sessions.get(id).map(|session| session.tab) else {
        return false
    };

    app.tab_list
        .tab_by_id(tab_id)
        .map(|tab| tab.shells.iter().any(|shell| *shell != id))
        .unwrap_or(false)
}

fn close_other_shells(app: &mut App, keep: ShellId) {
    let Some(tab_id) = app.shell_sessions.get(keep).map(|session| session.tab) else {
        return
    };

    app.close_other_shells(tab_id, keep);
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

/// Which mouse mode hire asks the outer terminal for.
///
/// The child's mode is kept as it is: with tracking on, its wheel and clicks
/// work (hire forwards them); with it off, hire captures nothing so the
/// terminal's native wheel and text selection keep working. Only the scroll
/// mode needs the wheel for hire, and then press reporting is enough (a
/// richer mode the child asked for is kept).
fn outer_mouse_mode(
    child: MouseProtocolMode,
    scroll_mode: bool,
) -> MouseProtocolMode {
    if scroll_mode && child == MouseProtocolMode::None {
        MouseProtocolMode::Press
    } else {
        child
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
/// when it speaks kitty. Inside tmux the sequences would need tmux passthrough
/// wrapping, which is not done yet, so nothing is offered there.
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
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
        open_test_shell_from(app, ShellOrigin::Browser)
    }

    /// Mount a session which was opened from `origin`.
    fn open_test_shell_from(app: &mut App, origin: ShellOrigin) -> ShellId {
        let tab_id = app.tab_list.current_tab().id;
        let id = app
            .shell_sessions
            .open(
                tab_id,
                "/bin/sh",
                Path::new("/"),
                GraphicsSupport::disabled(),
                origin,
            )
            .unwrap();

        let tab = app.tab_list.current_tab_mut();
        tab.shells.push(id);
        tab.active_shell = Some(id);

        id
    }

    #[test]
    fn test_attach_active_returns_to_the_last_used_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        open_test_shell(&mut app);
        open_test_shell(&mut app);

        attach(&mut app, first).unwrap();
        detach(&mut app);

        attach_active(&mut app).unwrap();

        assert_eq!(app.selected_block.shell_id(), Some(first));
        assert_eq!(app.tab_list.current_tab().active_shell, Some(first));
        assert_eq!(app.tab_list.current_tab().shells.len(), 3);
        // A shell of the current tab does not borrow a tab.
        assert!(app.shell_return_tab.is_none());
    }

    #[test]
    fn test_attach_active_borrows_the_last_shell_of_another_tab() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first_tab = app.tab_list.current_tab().id;
        let id = open_test_shell(&mut app);

        // Attach it once so it becomes the most recently used shell.
        attach(&mut app, id).unwrap();
        detach(&mut app);

        app.tab_list.push(app.path.clone(), false);
        let empty_index = app.tab_list.len() - 1;
        app.tab_list.set_current(empty_index);
        let empty_tab = app.tab_list.current_tab().id;

        attach_active(&mut app).unwrap();

        assert_eq!(app.selected_block.shell_id(), Some(id));
        // The view is rendered as part of the shell's own tab.
        assert_eq!(app.tab_list.current(), 0);
        assert_eq!(app.tab_list.current_tab().id, first_tab);
        assert_eq!(app.shell_return_tab, Some(empty_tab));

        detach(&mut app);

        assert_eq!(app.tab_list.current(), empty_index);
        assert_eq!(app.tab_list.current_tab().id, empty_tab);
        assert!(app.shell_return_tab.is_none());
    }

    #[test]
    fn test_detach_refreshes_the_borrowed_shell_tab() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let dir = temp_dir("borrowed-refresh");
        fs::write(dir.join("b.txt"), "").unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();
        app.file_search_sync(String::from("b.txt"), true).unwrap();

        // The tab only stores its directory when it is left, so save it here:
        // the test never switches tabs through the normal path.
        crate::key_event::tab::update_current_tab(&mut app);

        let shell_tab = app.tab_list.current_tab().id;
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();
        detach(&mut app);
        assert!(app.shell_return_tab.is_none());

        app.tab_list.push(app.path.clone(), false);
        let other_index = app.tab_list.len() - 1;
        app.tab_list.set_current(other_index);
        let other_tab = app.tab_list.current_tab().id;

        attach(&mut app, id).unwrap();
        assert_eq!(app.shell_return_tab, Some(other_tab));

        // The shell writes a file which sorts before the selected one.
        fs::write(dir.join("a.txt"), "").unwrap();

        detach(&mut app);
        assert_eq!(app.tab_list.current(), other_index);

        // The refresh ran while the shell's tab was still current, so the saved
        // selection is right even though the directory changed meanwhile.
        switch_to_tab(&mut app, shell_tab).unwrap();
        assert_eq!(
            app.get_file_saver().map(|file| file.name.as_str()),
            Some("b.txt"),
        );
    }

    #[test]
    fn test_attach_active_without_a_shell_reports_an_error() {
        let mut app = App::default();

        assert!(attach_active(&mut app).is_err());
        assert!(matches!(app.selected_block, Block::Browser(_)));
    }

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

    #[test]
    fn test_prefix_o_kills_the_other_shells_after_confirmation() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell(&mut app);
        attach(&mut app, second).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::empty());

        assert_eq!(app.shell_pending_kill_others, Some(second));

        // Nothing is killed before the confirmation.
        assert_eq!(app.tab_list.current_tab().shells.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 2);

        press(&mut app, KeyCode::Char('y'), KeyModifiers::empty());

        assert!(app.shell_pending_kill_others.is_none());
        assert_eq!(app.selected_block.shell_id(), Some(second));
        assert_eq!(app.tab_list.current_tab().shells, vec![second]);
        assert_eq!(app.tab_list.current_tab().active_shell, Some(second));
        assert!(!app.shell_sessions.is_alive(first));
        assert!(app.shell_sessions.is_alive(second));
    }

    #[test]
    fn test_prefix_o_cancel_keeps_the_other_shells() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        open_test_shell(&mut app);
        let second = open_test_shell(&mut app);
        attach(&mut app, second).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::empty());
        press(&mut app, KeyCode::Char('n'), KeyModifiers::empty());

        assert!(app.shell_pending_kill_others.is_none());
        assert!(app.selected_block.is_shell());
        assert_eq!(app.tab_list.current_tab().shells.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 2);
    }

    #[test]
    fn test_prefix_o_without_other_shells_does_not_ask() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::empty());

        assert!(app.shell_pending_kill_others.is_none());
        assert!(app.selected_block.is_shell());
        assert_eq!(app.tab_list.current_tab().shells, vec![id]);
    }

    #[test]
    fn test_prefix_leaves_the_scroll_mode() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('['), KeyModifiers::SHIFT);

        assert!(app.shell_scroll_mode);

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);

        assert!(!app.shell_scroll_mode);
        assert!(app.shell_prefix_active);

        press(&mut app, KeyCode::Char('d'), KeyModifiers::empty());

        assert!(!app.selected_block.is_shell());
    }

    #[test]
    fn test_outer_mouse_mode_policy() {
        use MouseProtocolMode::{AnyMotion, None as NoMouse, Press};

        assert_eq!(outer_mouse_mode(NoMouse, false), NoMouse);
        assert_eq!(outer_mouse_mode(NoMouse, true), Press);
        assert_eq!(outer_mouse_mode(Press, false), Press);
        assert_eq!(outer_mouse_mode(AnyMotion, true), AnyMotion);
    }

    fn wheel(up: bool, shift: bool) -> MouseEvent {
        MouseEvent {
            kind: if up {
                MouseEventKind::ScrollUp
            } else {
                MouseEventKind::ScrollDown
            },
            column: 2,
            row: 2,
            modifiers: if shift {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::empty()
            },
        }
    }

    fn feed(app: &App, id: ShellId, text: &str) {
        let session = app.shell_sessions.get(id).unwrap();
        session.parser().process(text.as_bytes());
    }

    #[test]
    fn test_wheel_goes_to_the_child_unless_shift() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();
        app.shell_area = ratatui::layout::Rect::new(0, 0, 40, 10);
        app.shell_sessions.get_mut(id).unwrap().resize(4, 20).unwrap();
        feed(&app, id, "\x1b[?1000h");
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n");

        // The child asked for the mouse: a plain wheel is forwarded, not
        // scrolled by hire.
        handle_view_mouse(wheel(true, false), &mut app).unwrap();

        assert!(!app.shell_scroll_mode);
        assert_eq!(app.shell_sessions.get(id).unwrap().parser().screen().scrollback(), 0);

        // Shift + wheel is hire's: it scrolls and opens the mode.
        handle_view_mouse(wheel(true, true), &mut app).unwrap();

        assert!(app.shell_scroll_mode);
        assert!(app.shell_sessions.get(id).unwrap().parser().screen().scrollback() > 0);
    }

    #[test]
    fn test_wheel_needs_shift_or_the_scroll_mode() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();
        app.shell_area = ratatui::layout::Rect::new(0, 0, 40, 10);
        app.shell_sessions.get_mut(id).unwrap().resize(4, 20).unwrap();
        feed(&app, id, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n");

        // No mouse mode, no scroll mode: the event is dropped (in a real
        // terminal it never reaches hire).
        handle_view_mouse(wheel(true, false), &mut app).unwrap();
        assert!(!app.shell_scroll_mode);

        // Open the mode: the wheel now scrolls freely, with or without Shift.
        scroll::enter(&mut app, id);
        handle_view_mouse(wheel(true, false), &mut app).unwrap();
        assert!(app.shell_scroll_mode);
        assert!(app.shell_sessions.get(id).unwrap().parser().screen().scrollback() > 0);
    }

    #[test]
    fn test_pin_attaches_the_shell_from_another_tab() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('*'), KeyModifiers::SHIFT);

        assert_eq!(app.pinned_shell, Some(id));

        detach(&mut app);
        app.tab_list.push(app.path.clone(), false);
        let other_index = app.tab_list.len() - 1;
        app.tab_list.set_current(other_index);
        let other_tab = app.tab_list.current_tab().id;

        attach_pinned(&mut app).unwrap();

        assert_eq!(app.selected_block.shell_id(), Some(id));
        assert_eq!(app.shell_return_tab, Some(other_tab));

        // Detaching returns to the tab the pinned shell was borrowed from.
        detach(&mut app);
        assert_eq!(app.tab_list.current(), other_index);

        // `C-s b` attaches the pinned shell from the view of another shell
        // as well.
        let second = open_test_shell(&mut app);
        attach(&mut app, second).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('b'), KeyModifiers::empty());

        assert_eq!(app.selected_block.shell_id(), Some(id));
    }

    #[test]
    fn test_pin_toggles_and_is_forgotten_with_the_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('*'), KeyModifiers::SHIFT);
        assert_eq!(app.pinned_shell, Some(id));

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('*'), KeyModifiers::SHIFT);
        assert_eq!(app.pinned_shell, None);

        toggle_pin(&mut app, id);
        assert_eq!(app.pinned_shell, Some(id));

        // Removing the shell drops the pin: `C-s b` then reports it instead of
        // pointing at a dead session.
        detach(&mut app);
        let tab_id = app.shell_sessions.get(id).unwrap().tab;
        app.remove_shell_from_tab(tab_id, id);
        app.shell_sessions.close(id);

        assert_eq!(app.pinned_shell, None);
        assert!(attach_pinned(&mut app).is_err());
    }

    #[test]
    fn test_attach_pinned_without_a_pin_reports_an_error() {
        let mut app = App::default();

        assert!(attach_pinned(&mut app).is_err());
        assert!(matches!(app.selected_block, Block::Browser(_)));
    }

    #[test]
    fn test_menu_opened_from_a_shell_returns_to_it() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();

        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('t'), KeyModifiers::empty());

        assert_eq!(app.shell_menu_origin, Some(id));
        assert!(app.switch_case.is_some());
        assert!(matches!(app.selected_block, Block::CommandLine(_, _)));

        assert!(app.leave_shell_menu());

        assert_eq!(app.selected_block.shell_id(), Some(id));
        assert_eq!(app.shell_menu_origin, None);
        assert!(app.switch_case.is_none());
    }

    #[test]
    fn test_menu_opened_from_the_browser_has_no_origin() {
        let mut app = App::default();

        shell_operation(&mut app).unwrap();

        assert_eq!(app.shell_menu_origin, None);
        assert!(!app.leave_shell_menu());
    }

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

    #[test]
    fn test_shell_opened_from_the_browser_returns_to_the_browser() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let other = open_test_shell(&mut app);
        let shell = open_test_shell(&mut app);
        attach(&mut app, shell).unwrap();

        close_shell(&mut app, shell);

        assert!(matches!(app.selected_block, Block::Browser(_)));
        assert!(!app.shell_pending_browser);
        assert_eq!(app.tab_list.current_tab().shells, vec![other]);
    }

    #[test]
    fn test_shell_opened_from_a_shell_returns_to_it() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell_from(&mut app, ShellOrigin::Shell);
        attach(&mut app, second).unwrap();

        close_shell(&mut app, second);

        assert_eq!(app.selected_block.shell_id(), Some(first));
        assert!(!app.shell_pending_browser);
    }

    /// The browser is one place: a detach marks every shell which is running
    /// at that moment, while a shell opened afterwards starts clean.
    #[test]
    fn test_detach_marks_every_running_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell(&mut app);
        attach(&mut app, first).unwrap();

        detach(&mut app);

        for id in [first, second] {
            assert!(
                app.shell_sessions.get(id).unwrap().returned_to_browser,
                "the running shell must remember the browser",
            );
        }

        let later = open_test_shell(&mut app);

        assert!(!app.shell_sessions.get(later).unwrap().returned_to_browser);
    }

    #[test]
    fn test_a_detach_from_another_shell_counts_for_the_new_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell(&mut app);

        // Attach the first shell and detach there; the second one is running
        // all along and never owned the view.
        attach(&mut app, first).unwrap();
        detach(&mut app);

        attach(&mut app, second).unwrap();
        close_shell(&mut app, second);

        assert_eq!(app.selected_block.shell_id(), Some(first));
        assert!(app.shell_pending_browser);
    }

    #[test]
    fn test_closing_a_shell_which_had_returned_to_the_browser_asks() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell(&mut app);
        attach(&mut app, second).unwrap();

        // Back to the browser once, then in front again.
        detach(&mut app);
        attach(&mut app, second).unwrap();

        close_shell(&mut app, second);

        // The view falls back to the tab's other shell while it asks.
        assert_eq!(app.selected_block.shell_id(), Some(first));
        assert!(app.shell_pending_browser);

        press(&mut app, KeyCode::Char('y'), KeyModifiers::empty());

        assert!(matches!(app.selected_block, Block::Browser(_)));
        assert!(!app.shell_pending_browser);
    }

    #[test]
    fn test_the_browser_question_is_cancelled_by_any_other_key() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let first = open_test_shell(&mut app);
        let second = open_test_shell(&mut app);
        attach(&mut app, second).unwrap();
        detach(&mut app);
        attach(&mut app, second).unwrap();

        close_shell(&mut app, second);
        press(&mut app, KeyCode::Char('n'), KeyModifiers::empty());

        assert_eq!(app.selected_block.shell_id(), Some(first));
        assert!(!app.shell_pending_browser);
    }

    /// A shell which is the tab's only one always goes back to the browser,
    /// even when the user had been there during its life.
    #[test]
    fn test_the_only_shell_of_a_tab_returns_to_the_browser() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = open_test_shell(&mut app);
        attach(&mut app, id).unwrap();
        detach(&mut app);
        attach(&mut app, id).unwrap();

        close_shell(&mut app, id);

        assert!(matches!(app.selected_block, Block::Browser(_)));
        assert!(!app.shell_pending_browser);
        assert!(app.tab_list.current_tab().shells.is_empty());
    }

    #[test]
    fn test_an_exited_shell_from_the_browser_returns_to_the_browser() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let other = open_test_shell(&mut app);
        let shell = open_test_shell(&mut app);
        attach(&mut app, shell).unwrap();

        app.shell_sessions
            .get_mut(shell)
            .unwrap()
            .write(b"exit\n")
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);

        while app.selected_block.shell_id() == Some(shell) {
            assert!(Instant::now() < deadline, "the shell did not exit");
            app.handle_shell_events();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(matches!(app.selected_block, Block::Browser(_)));
        assert!(!app.shell_pending_browser);
        assert_eq!(app.tab_list.current_tab().shells, vec![other]);
    }
}
