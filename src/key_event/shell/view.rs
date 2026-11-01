// Shell view.
//
// Opening, attaching, detaching and killing shell sessions, plus the key
// handling and the numbered selection used while a shell is attached.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::App,
    error::{AppResult, ErrorType, NotFoundType},
    key_event::TabId,
    rt_error,
    utils::{Block, ShellId},
};

use super::{default_shell, encode_key};

/// How long a partial shell number is kept before it is committed.
const DIGIT_COMMIT_DELAY: Duration = Duration::from_millis(450);

pub fn open_shell(app: &mut App) -> AppResult<()> {
    let program = default_shell(app)?;
    let tab_id = app.tab_list.current_tab().id;
    let cwd = app.path.to_owned();

    let id = app.shell_sessions.open(tab_id, &program, &cwd)?;

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
    app.shell_notice = None;
    app.clear_shell_digits();
    app.selected_block = Block::Shell(id);
    app.clear_screen = true;

    Ok(())
}

pub fn detach(app: &mut App) {
    let in_root = app.root();

    app.shell_prefix_active = false;
    app.shell_pending_kill = None;
    app.shell_notice = None;
    app.clear_shell_digits();
    app.selected_block = Block::Browser(in_root);
    app.clear_screen = true;
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

    // A kill confirmation is waiting for `y`.
    if let Some(pending) = app.shell_pending_kill {
        app.shell_pending_kill = None;

        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            close_shell(app, pending);
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
