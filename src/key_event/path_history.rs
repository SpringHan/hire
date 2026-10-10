// Path history operation.

use std::any::Any;

use ratatui::style::{Modifier, Stylize};
use ratatui::text::{Line, Span, Text};

use super::switch::{SwitchCase, SwitchCaseData, SwitchStruct};
use crate::error::{AppResult, ErrorType, NotFoundType};
use crate::utils::CmdContent;
use crate::app::App;
use crate::rt_error;

/// The maximum number of paths that `App::path_history` can store.
pub const PATH_HISTORY_LIMIT: usize = 30;

/// The state of the path history page, which stores the keys pressed by user
/// to form the number of the target path.
#[derive(Clone, Default)]
pub struct PathHistoryState {
    selecting: Vec<u8>,
}

impl SwitchStruct for PathHistoryState {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl PathHistoryState {
    fn push(&mut self, digit: u8) {
        self.selecting.push(digit);
    }

    fn len(&self) -> usize {
        self.selecting.len()
    }

    /// The number formed by all the keys pressed so far.
    fn number(&self) -> usize {
        self.selecting.iter()
            .fold(0usize, |number, digit| number * 10 + *digit as usize)
    }
}

impl App<'_> {
    /// Record the path before jumping to a new path.
    ///
    /// The path will be removed first when it's already in the history, so the
    /// last item is always the newest path. The oldest path will be dropped
    /// when the history is full.
    pub fn record_path_history(&mut self) {
        let path = self.path.to_owned();

        if let Some(idx) = self.path_history.iter().position(|item| item == &path) {
            self.path_history.remove(idx);
        }

        self.path_history.push(path);

        if self.path_history.len() > PATH_HISTORY_LIMIT {
            self.path_history.remove(0);
        }
    }
}

/// Jump to the last path in the path history, which is also the path before
/// the last jump.
pub fn goto_prev_path(app: &mut App) -> AppResult<()> {
    let prev_path = match app.path_history.pop() {
        Some(path) => path,
        None => rt_error!("There's no path in the history"),
    };

    app.record_path_history();
    app.goto_dir(prev_path, None)?;

    Ok(())
}

/// Open the path history page.
pub fn path_history_operation(app: &mut App) {
    let state = PathHistoryState::default();

    SwitchCase::new(
        app,
        path_history_switch,
        true,
        generate_msg(app, &state),
        SwitchCaseData::Struct(Box::new(state))
    );
}

/// Handle the keys pressed on the path history page.
fn path_history_switch(
    app: &mut App,
    key: char,
    data: SwitchCaseData
) -> AppResult<bool>
{
    let mut state = if let SwitchCaseData::Struct(data) = data {
        match data.as_any().downcast_ref::<PathHistoryState>() {
            Some(state) => state.to_owned(),
            None => panic!(
                "[1] Unexpected error at path_history_switch in path_history.rs."
            ),
        }
    } else {
        panic!(
            "[2] Unexpected error at path_history_switch in path_history.rs."
        )
    };

    let digit = match key.to_digit(10) {
        Some(digit) => digit as u8,
        // Any key that is not a number closes the page.
        None => return Ok(true),
    };

    state.push(digit);

    let total = app.path_history.len();
    let number = state.number();

    // The number of the target path may be made up of more than one key,
    // so the pressed keys should only be regarded as a complete number when
    // it cannot be extended to another valid one.
    if !can_extend(number, state.len(), total) {
        if number < 1 || number > total {
            return Err(ErrorType::NotFound(NotFoundType::None).pack())
        }

        jump_to_history_path(app, number - 1)?;

        return Ok(true)
    }

    SwitchCase::new(
        app,
        path_history_switch,
        true,
        generate_msg(app, &state),
        SwitchCaseData::Struct(Box::new(state))
    );

    Ok(false)
}

/// Check whether the keys pressed so far can be extended to form another
/// valid number that is not larger than `total`.
fn can_extend(number: usize, digits: usize, total: usize) -> bool {
    if digits >= total.to_string().len() {
        return false
    }

    (0..=9).any(|digit| {
        let next = number * 10 + digit;
        next >= 1 && next <= total
    })
}

/// Jump to the path at `idx` (started from 0) in the path history, and move
/// the path before jumping to the last place of the history.
fn jump_to_history_path(app: &mut App, idx: usize) -> AppResult<()> {
    if idx >= app.path_history.len() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let target_path = app.path_history.remove(idx);
    app.record_path_history();
    app.goto_dir(target_path, None)?;

    Ok(())
}

fn generate_msg(app: &App, state: &PathHistoryState) -> CmdContent {
    let mut msg = Text::raw("[Esc] close the page  [01-30] jump to the path");
    msg.push_line("");

    if app.path_history.is_empty() {
        msg.push_line("There's no path in the history");
        return CmdContent::Text(msg)
    }

    let selecting = state.number();

    for (idx, path) in app.path_history.iter().enumerate() {
        let mut line = Line::default();
        line.push_span(
            Span::raw(format!("[{:02}]", idx + 1))
                .add_modifier(
                    if !state.selecting.is_empty() && selecting == idx + 1 {
                        Modifier::REVERSED
                    } else {
                        Modifier::empty()
                    }
                )
        );
        line.push_span(format!(" {}", path.to_string_lossy()));

        msg.push_line(line);
    }

    CmdContent::Text(msg)
}
