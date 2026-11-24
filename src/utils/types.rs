//! Types shared by the app: blocks, cursors and preview content.

use std::collections::HashMap;

use ratatui::{text::Text, widgets::ListState};

/// The identity of a shell session mounted on a tab.
///
/// The value is opaque: it is only ever looked up in `ShellSessions`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ShellId(pub u64);

/// The cursor position inside a piece of editable text.
///
/// `Index` is a **byte offset** into the UTF-8 text and always lies on a
/// character boundary, so a multi-byte character is moved and deleted whole.
/// `End` is the point after the last character; `None` means not editable.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CursorPos {
    Index(usize),
    End,
    None
}

/// The command-line content: `String` while it is editable, `Text` when it is
/// styled and read-only.
#[derive(Clone)]
pub enum CmdContent {
    String(String),
    Text(Text<'static>)
}

/// A block which can be selected; `Browser`'s flag says whether the current
/// directory is the root directory.
pub enum Block {
    Browser(bool),
    CommandLine(CmdContent, CursorPos),

    /// The attached shell view owns the input; the value is the shell id.
    Shell(ShellId),
}

impl Block {
    pub fn shell_id(&self) -> Option<ShellId> {
        if let Self::Shell(id) = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn is_shell(&self) -> bool {
        matches!(self, Self::Shell(_))
    }
}

/// Store file's name and whether it's a directory.
#[derive(Clone)]
pub struct MarkedFiles {
    pub files: HashMap<String, bool>,
}

pub struct ItemIndex {
    pub parent: ListState,
    pub current: ListState,
    pub child: ListState
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchFile {
    Parent,
    Current,
    Child
}

/// Enumeration for File Content
#[derive(Clone, PartialEq, Eq)]
pub enum FileContent {
    Text(Text<'static>),
    Image,
    None
}

impl Default for MarkedFiles {
    fn default() -> Self {
        MarkedFiles {
            files: HashMap::new()
        }
    }
}

impl CmdContent {
    pub fn get_str(&self) -> &String {
        match *self {
            CmdContent::String(ref _str) => _str,
            CmdContent::Text(_) => panic!(
                "Unknow error when converting command line content to String"
            ),
        }
    }

    pub fn into_text(&self) -> Text<'_> {
        match *self {
            CmdContent::Text(ref text) => text.to_owned(),
            CmdContent::String(ref _str) => Text::raw(_str.to_owned()),
        }
    }

    pub fn get(&self) -> &String {
        if let Self::String(_ref) = self {
            _ref
        } else {
            panic!("Unknown error when getting mutable ref of CmdContent!")
        }
    }

    /// The mutable `String`; panics for the `Text` variant.
    pub fn get_mut(&mut self) -> &mut String {
        if let Self::String(_ref) = self {
            _ref
        } else {
            panic!("Unknown error when getting mutable ref of CmdContent!")
        }
    }
}

impl Default for ItemIndex {
    fn default() -> ItemIndex {
        ItemIndex {
            parent: ListState::default(),
            current: ListState::default(),
            child: ListState::default()
        }
    }
}

impl ItemIndex {
    pub fn parent_selected(&self) -> Option<usize> {
        self.parent.selected()
    }

    pub fn current_selected(&self) -> Option<usize> {
        self.current.selected()
    }

    pub fn child_selected(&self) -> Option<usize> {
        self.child.selected()
    }

    pub fn parent_select(&mut self, idx: Option<usize>) {
        self.parent.select(idx);
    }

    pub fn current_select(&mut self, idx: Option<usize>) {
        self.current.select(idx);
    }

    pub fn child_select(&mut self, idx: Option<usize>) {
        self.child.select(idx);
    }
}

impl FileContent {
    pub fn is_some(&self) -> bool {
        if *self == Self::None {
            return false
        }

        true
    }

    pub fn reset(&mut self) {
        *self = Self::None;
    }
}
