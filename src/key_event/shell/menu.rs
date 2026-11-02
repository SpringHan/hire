// The shell menu.
//
// Lists the shells mounted on the current tab. Numbers attach a shell; `x`
// switches to kill mode and `D` clears every shell of the tab. The menu
// follows the `SwitchCase` convention used by the tab menu.

use ratatui::{
    style::{Modifier, Stylize},
    text::{Line, Span, Text},
};

use crate::{
    app::App,
    error::AppResult,
    key_event::{SwitchCase, SwitchCaseData, TabId},
    utils::{CmdContent, ShellId},
};

use super::view::{attach, open_shell};
use crate::key_event::switch::SwitchStruct;

#[derive(Clone, Default)]
pub struct ShellMenuState {
    /// The next number kills a shell instead of attaching it.
    deleting: bool,

    /// Digits typed so far.
    selecting: String,

    /// The tab whose shells are about to be cleared; waits for `y`.
    ///
    /// The id is remembered instead of a boolean so the confirmation clears
    /// the tab it was opened for even when the tab list changed meanwhile.
    clear_pending: Option<TabId>,
}

impl SwitchStruct for ShellMenuState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn shell_operation(app: &mut App) -> AppResult<()> {
    let state = ShellMenuState::default();

    SwitchCase::new(
        app,
        shell_switch,
        true,
        generate_msg(app, &state),
        SwitchCaseData::Struct(Box::new(state)),
    );

    Ok(())
}

fn shell_switch(
    app: &mut App,
    key: char,
    data: SwitchCaseData,
) -> AppResult<bool>
{
    let mut state = match data {
        SwitchCaseData::Struct(data) => data
            .as_any()
            .downcast_ref::<ShellMenuState>()
            .cloned()
            .unwrap_or_default(),
        _ => ShellMenuState::default(),
    };

    // A "clear every shell of this tab" request waits for `y`. Any other key
    // cancels it and brings the menu back.
    if let Some(tab_id) = state.clear_pending.take() {
        if matches!(key, 'y' | 'Y') {
            app.close_tab_shells(tab_id);
        }

        state.deleting = false;
        state.selecting.clear();
        reopen(app, state);

        return Ok(false)
    }

    match key {
        // Create a new shell on this tab and attach it. `open_shell` clears the
        // switch case itself, so this must not report `true`.
        'n' => {
            open_shell(app)?;
            return Ok(false)
        },

        'x' => {
            state.deleting = true;
            state.selecting.clear();
            reopen(app, state);
            return Ok(false)
        },

        // Kill every shell mounted on the current tab and keep the tab open.
        // This is the tab-level action; `C-s D` in the attached view does the
        // same thing while a shell owns the input.
        'D' => {
            let tab = app.tab_list.current_tab();

            if tab.shells.is_empty() {
                // Nothing to clear; the page already says so.
                reopen(app, state);
                return Ok(false)
            }

            state.clear_pending = Some(tab.id);
            state.deleting = false;
            state.selecting.clear();
            reopen(app, state);
            return Ok(false)
        },

        '0'..='9' => {
            if let Some(idx) = selected_index(app, &mut state, key) {
                let id = app.tab_list.current_tab().shells[idx];

                if state.deleting {
                    kill_shell(app, id);

                    if app.tab_list.current_tab().shells.is_empty() {
                        return Ok(true)
                    }

                    state.deleting = false;
                    reopen(app, state);
                } else {
                    attach(app, id)?;
                }

                return Ok(false)
            }

            reopen(app, state);
            return Ok(false)
        },

        _ => (),
    }

    Ok(true)
}

fn reopen(app: &mut App, state: ShellMenuState) {
    let msg = generate_msg(app, &state);

    SwitchCase::new(
        app,
        shell_switch,
        true,
        msg,
        SwitchCaseData::Struct(Box::new(state)),
    );
}

fn kill_shell(app: &mut App, id: ShellId) {
    let tab_id = app.shell_sessions.get(id).map(|session| session.tab);

    app.shell_sessions.close(id);

    if let Some(tab_id) = tab_id {
        app.remove_shell_from_tab(tab_id, id);
    }
}

/// Read one digit of a shell number. Returns the index once the typed number
/// can no longer grow into a valid one.
fn selected_index(
    app: &App,
    state: &mut ShellMenuState,
    key: char,
) -> Option<usize>
{
    let len = app.tab_list.current_tab().shells.len();

    if len == 0 {
        return None
    }

    state.selecting.push(key);

    let value = match state.selecting.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            state.selecting.clear();
            return None
        },
    };

    if value >= len {
        state.selecting.clear();
        return None
    }

    // The width of the largest valid index, e.g. 2 for 12 shells.
    let width = (len - 1).to_string().len().max(1);

    if state.selecting.len() >= width || value.saturating_mul(10) > len - 1 {
        state.selecting.clear();
        return Some(value)
    }

    None
}

fn generate_msg(app: &App, state: &ShellMenuState) -> CmdContent {
    // While a confirmation is open the key list is irrelevant: every key either
    // confirms (`y`) or cancels.
    let mut text = if state.clear_pending.is_some() {
        Text::default()
    } else {
        Text::raw(
            "[n] new shell  [x] kill shell  [D] clear shells of this tab  \
             [0-9] attach shell  [Esc] close\n",
        )
    };

    if let Some(tab_id) = state.clear_pending {
        let count = app
            .tab_list
            .tab_by_id(tab_id)
            .map(|tab| tab.shells.len())
            .unwrap_or(0);

        text.push_line(
            Line::raw(format!(
                "Kill all {count} shell(s) of this tab? (y/N)"
            ))
            .red(),
        );
    }

    if state.deleting {
        text.push_line(Line::raw("Delete mode: type the shell number!").red());
    }

    if !state.selecting.is_empty() {
        text.push_line(
            Line::raw(format!("Selecting shell: {}_", state.selecting)).yellow(),
        );
    }

    text.push_line("");

    let tab = app.tab_list.current_tab();

    if tab.shells.is_empty() {
        text.push_line(Line::raw("No shell is mounted on this tab."));
        return CmdContent::Text(text)
    }

    for (idx, id) in tab.shells.iter().enumerate() {
        // The foreground program, not the shell: `vim` and `zsh` sessions must
        // be distinguishable at a glance.
        let (title, cwd, alive) = match app.shell_sessions.get(*id) {
            Some(session) => (
                session.display_title(),
                session.meta.cwd.to_string_lossy().into_owned(),
                session.alive,
            ),
            None => (String::from("?"), String::new(), false),
        };

        let marker = if tab.active_shell == Some(*id) {
            "*"
        } else if !alive {
            "!"
        } else {
            " "
        };

        let mut line = Line::default();
        line.push_span(
            Span::raw(format!("[{}] ", idx))
                .add_modifier(Modifier::BOLD),
        );
        line.push_span(Span::raw(format!("{marker} {title}  {cwd}")));

        if !alive {
            line.push_span(Span::raw("  (exited)").red());
        }

        text.push_line(line);
    }

    CmdContent::Text(text)
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::{
        key_event::{shell::GraphicsSupport, switch::switch_match},
        utils::Block,
    };

    use super::*;

    /// Mount a `/bin/sh` session on the current tab, without attaching it.
    fn mount_shell(app: &mut App) -> ShellId {
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

    /// Open the shell menu and press `keys` one after another.
    fn menu(app: &mut App, keys: &[char]) {
        shell_operation(app).unwrap();

        for key in keys {
            // The dispatcher clones the active case and leaves it stored while
            // the handler returns `false`.
            let case = app.switch_case.clone().expect("the shell menu is open");
            switch_match(app, case, *key).unwrap();
        }
    }

    /// The text the menu page currently shows.
    fn menu_text(app: &App) -> String {
        let Block::CommandLine(CmdContent::Text(text), _) = &app.selected_block
        else {
            return String::new()
        };

        text.lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    /// `D` asks before clearing every shell; `y` then kills them all and keeps
    /// the tab (and the menu) open.
    #[test]
    fn test_clear_shells_asks_before_killing_every_shell() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        mount_shell(&mut app);
        let last = mount_shell(&mut app);

        menu(&mut app, &['D']);

        assert!(app.switch_case.is_some());
        assert!(menu_text(&app).contains("Kill all 2 shell(s)"));

        // Nothing is killed before the confirmation.
        assert_eq!(app.tab_list.current_tab().shells.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 2);

        let case = app.switch_case.clone().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        // The tab survives, its shells are gone and the menu stays open.
        assert_eq!(app.tab_list.len(), 1);
        assert!(app.tab_list.current_tab().shells.is_empty());
        assert_eq!(app.shell_sessions.alive_count(), 0);
        assert!(!app.shell_sessions.is_alive(last));
        assert!(app.switch_case.is_some());
    }

    /// Any other key cancels the confirmation.
    #[test]
    fn test_clear_shells_cancel_keeps_them() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        mount_shell(&mut app);

        menu(&mut app, &['D', 'n']);

        assert!(app.switch_case.is_some());
        assert!(!menu_text(&app).contains("Kill all"));
        assert_eq!(app.tab_list.current_tab().shells.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 1);
    }

    /// `D` on a tab without shells does not ask, it just says there is nothing
    /// to clear.
    #[test]
    fn test_clear_shells_without_shells_does_not_ask() {
        let mut app = App::default();

        menu(&mut app, &['D']);

        assert!(app.switch_case.is_some());
        assert!(!menu_text(&app).contains("Kill all"));
        assert!(app.tab_list.current_tab().shells.is_empty());
    }
}
