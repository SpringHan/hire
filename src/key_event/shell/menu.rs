// The shell menu.
//
// Lists the shells mounted on the current tab. Numbers attach a shell; `x`
// switches to kill mode. The menu follows the `SwitchCase` convention used by
// the tab menu.

use ratatui::{
    style::{Modifier, Stylize},
    text::{Line, Span, Text},
};

use crate::{
    app::App,
    error::AppResult,
    key_event::{SwitchCase, SwitchCaseData},
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
    let mut text = Text::raw(
        "[n] new shell  [x] kill shell  [0-9] attach shell  [Esc] close\n",
    );

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
        let (title, cwd, alive) = match app.shell_sessions.get(*id) {
            Some(session) => (
                session.meta.title.clone(),
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
