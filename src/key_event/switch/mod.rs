//! The menu state machine: a switch case, its body and the picker list.

mod traits;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span, Text},
};

use crate::{
    app::App,
    error::AppResult,
    utils::{display_width, Block, CmdContent, CursorPos},
};

pub use traits::SwitchStruct;

type FuncPointer = fn(&mut App, char, SwitchCaseData) -> AppResult<bool>;

pub enum SwitchCaseData {
    None,
    Char(char),
    Bool(bool),
    Struct(Box<dyn SwitchStruct>)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuPresentation {
    /// A centered box over the dimmed page, with free-form text.
    Dialog,

    /// A centered box over the dimmed page whose body is a list of rows.
    Picker,

    /// The bottom command line only.
    Inline,
}

/// What a menu shows.
///
/// A list body is always a picker; a text body follows the presentation.
pub enum MenuBody {
    Text(CmdContent),
    List(MenuList),
}

impl From<CmdContent> for MenuBody {
    fn from(content: CmdContent) -> Self {
        Self::Text(content)
    }
}

impl From<MenuList> for MenuBody {
    fn from(list: MenuList) -> Self {
        Self::List(list)
    }
}

/// A list-shaped menu: hint lines, selectable entries and notices.
///
/// Only entries are selectable, so the key handler owns the selection by entry
/// index and the renderer only maps it to a row to scroll into view.
#[derive(Clone, Default)]
pub struct MenuList {
    header: Vec<Line<'static>>,
    rows: Vec<MenuRow>,
    footer: Vec<Line<'static>>,

    /// The highlighted entry, as an index into the entries (not the rows).
    selected: Option<usize>,
}

#[derive(Clone)]
pub enum MenuRow {
    /// A section title or a continuation line: skipped by the selection.
    Line(Line<'static>),

    Entry(MenuEntry),
}

#[derive(Clone)]
pub struct MenuEntry {
    /// The key shown in the left column, e.g. `1` or `g`.
    pub key: String,

    pub row: Line<'static>,
}

impl MenuEntry {
    pub fn new(key: impl Into<String>, row: impl Into<Line<'static>>) -> Self {
        Self {
            key: key.into(),
            row: row.into(),
        }
    }
}

impl MenuList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn header(mut self, line: impl Into<Line<'static>>) -> Self {
        self.header.push(line.into());
        self
    }

    /// Add a non-selectable line between the entries.
    pub fn line(mut self, line: impl Into<Line<'static>>) -> Self {
        self.rows.push(MenuRow::Line(line.into()));
        self
    }

    pub fn entry(
        mut self,
        key: impl Into<String>,
        row: impl Into<Line<'static>>
    ) -> Self
    {
        self.rows.push(MenuRow::Entry(MenuEntry::new(key, row)));
        self
    }

    /// `entry` is an index into the entries, not into the rows.
    pub fn select(mut self, entry: Option<usize>) -> Self {
        self.selected = entry;
        self
    }

    pub fn header_lines(&self) -> &[Line<'static>] {
        &self.header
    }

    pub fn rows(&self) -> &[MenuRow] {
        &self.rows
    }

    pub fn footer_lines(&self) -> &[Line<'static>] {
        &self.footer
    }

    /// The row the selected entry occupies, for the renderer's list state.
    pub fn selected_row(&self) -> Option<usize> {
        let selected = self.selected?;

        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, MenuRow::Entry(_)))
            .nth(selected)
            .map(|(idx, _)| idx)
    }

    pub fn entry_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row, MenuRow::Entry(_)))
            .count()
    }

    /// The widest key, so the renderer can align the key column.
    pub fn key_width(&self) -> usize {
        self.rows
            .iter()
            .filter_map(|row| match row {
                MenuRow::Entry(entry) => Some(display_width(&entry.key)),
                MenuRow::Line(_) => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// The line an entry is drawn as before the horizontal scroll is applied.
    ///
    /// The space after the key stays plain so a reversed key is a bracketed
    /// token, not a block which swallows the padding.
    pub fn entry_line(
        &self,
        entry: &MenuEntry,
        key_width: usize,
        key_style: Style
    ) -> Line<'static>
    {
        let mut line = entry.row.clone();
        line.spans.insert(
            0,
            Span::raw(" ")
        );
        line.spans.insert(
            0,
            Span::styled(
                format!("[{:>width$}]", entry.key, width = key_width),
                key_style
            )
        );
        line
    }

    fn push_notice(&mut self, message: String) {
        self.footer.push(
            Line::styled(message, Style::default().fg(Color::Red))
        );
    }

    /// The whole menu as plain text.
    ///
    /// The command-line block holds this, so states which only read the block
    /// (the quit guard, an error before the picker shows) still see the menu.
    fn to_text(&self, key_style: Style) -> CmdContent {
        let mut text = Text::default();

        for line in &self.header {
            text.push_line(line.clone());
        }

        let key_width = self.key_width();

        for row in &self.rows {
            match row {
                MenuRow::Line(line) => text.push_line(line.clone()),
                MenuRow::Entry(entry) => {
                    text.push_line(self.entry_line(entry, key_width, key_style));
                },
            }
        }

        for line in &self.footer {
            text.push_line(line.clone());
        }

        CmdContent::Text(text)
    }
}

#[derive(Clone)]
pub struct SwitchCase {
    func: FuncPointer,
    title: String,
    presentation: MenuPresentation,
    list: Option<MenuList>,
    data: SwitchCaseData,
}

impl SwitchCase {
    /// Open a menu page.
    ///
    /// `presentation` picks a box over the browser (`title` goes on its border)
    /// or the bottom command line only, where just the message fits and the
    /// title stays unused; a list body always makes a picker.
    pub fn new<B: Into<MenuBody>>(
        app: &mut App,
        func: FuncPointer,
        presentation: MenuPresentation,
        title: &str,
        body: B,
        data: SwitchCaseData
    )
    {
        if app.command_error {
            app.command_error = false;
        }

        let (text, list) = match body.into() {
            MenuBody::Text(content) => (content, None),
            MenuBody::List(list) => {
                (list.to_text(app.term_colors.executable_style), Some(list))
            },
        };

        let presentation = if list.is_some() {
            MenuPresentation::Picker
        } else {
            presentation
        };

        if presentation == MenuPresentation::Inline {
            app.expand_quit();
        } else {
            app.expand_init();
        }

        app.selected_block = Block::CommandLine(text, CursorPos::None);
        app.switch_case = Some(SwitchCase {
            func,
            title: title.to_owned(),
            presentation,
            list,
            data,
        });
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn presentation(&self) -> MenuPresentation {
        self.presentation
    }

    pub fn list(&self) -> Option<&MenuList> {
        self.list.as_ref()
    }

    /// Append an error to a picker; `false` when the body is plain text, so the
    /// caller writes the message to the command line itself.
    pub fn append_notice(&mut self, message: String) -> bool {
        let Some(list) = self.list.as_mut() else {
            return false
        };

        list.push_notice(message);
        true
    }
}

pub fn switch_match(
    app: &mut App,
    case: SwitchCase,
    key: char
) -> AppResult<()>
{
    let SwitchCase { func, data, .. } = case;

    // A handler which opened a new menu (it returned `false`) keeps the command
    // mode open, and a showing error must not be wiped by quitting it.
    if !app.command_error && func(app, key, data)? {
        app.quit_command_mode();
    }

    Ok(())
}
