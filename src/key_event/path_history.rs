// Path history operation.

use std::any::Any;

use ratatui::text::Line;

use super::switch::{
    MenuList, MenuPresentation, SwitchCase, SwitchCaseData, SwitchStruct,
};
use crate::error::{AppResult, ErrorType, NotFoundType};
use crate::app::App;
use crate::ui::key_hints;
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
        MenuPresentation::Picker,
        "Path History",
        generate_menu(app, &state),
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
        MenuPresentation::Picker,
        "Path History",
        generate_menu(app, &state),
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

/// The path history menu: a hint line and one entry per recorded path.
fn generate_menu(app: &App, state: &PathHistoryState) -> MenuList {
    let mut menu = MenuList::new()
        .header(key_hints(
            "[Esc] close the page  [01-30] jump to the path",
            app.term_colors.marked_style
        ))
        .header(Line::default());

    if app.path_history.is_empty() {
        return menu.header(Line::raw("There's no path in the history"))
    }

    for (idx, path) in app.path_history.iter().enumerate() {
        menu = menu.entry(
            format!("{:02}", idx + 1),
            Line::raw(path.to_string_lossy().into_owned())
        );
    }

    // The number being typed highlights the path it names; it is only shown
    // once it is a real entry, a half-typed number names nothing yet.
    let number = state.number();
    let selected = if !state.selecting.is_empty() &&
        number >= 1 && number <= app.path_history.len()
    {
        Some(number - 1)
    } else {
        None
    };

    menu.select(selected)
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::key_event::MenuRow;

    use super::*;

    fn app_with(paths: &[&str]) -> App<'static> {
        let mut app = App::default();
        app.path_history = paths.iter().map(PathBuf::from).collect();
        app
    }

    fn keys(menu: &MenuList) -> Vec<&str> {
        menu.rows()
            .iter()
            .filter_map(|row| match row {
                MenuRow::Entry(entry) => Some(entry.key.as_str()),
                MenuRow::Line(_) => None,
            })
            .collect()
    }

    /// The menu lists the recorded paths with 1-based keys and highlights the
    /// number being typed while it names an entry.
    #[test]
    fn test_menu_lists_paths_and_highlights_the_typed_number() {
        let app = app_with(&["/a", "/b", "/c"]);
        let menu = generate_menu(&app, &PathHistoryState::default());

        assert_eq!(keys(&menu), vec!["01", "02", "03"]);
        assert_eq!(menu.selected_row(), None);

        let mut state = PathHistoryState::default();
        state.push(2);

        assert_eq!(generate_menu(&app, &state).selected_row(), Some(1));
    }

    /// An empty history is a hint, not an empty list.
    #[test]
    fn test_empty_history_is_a_hint() {
        let app = App::default();
        let menu = generate_menu(&app, &PathHistoryState::default());

        assert_eq!(menu.entry_count(), 0);

        let header: String = menu
            .header_lines()
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        assert!(header.contains("no path"), "{header}");
    }
}
