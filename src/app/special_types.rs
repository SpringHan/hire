// Special types used in App structure.

use std::collections::HashMap;

use ratatui::{text::Text, widgets::ListState};

use crate::key_event::SwitchCase;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CursorPos {
    Index(usize),
    End,
    None
}

pub enum OptionFor {
    Switch(SwitchCase),
    None
}

/// The struct to store content in command line.
/// The `String` is used to store editable command.
/// The `Text` is used to store non-editable style text.
#[derive(Clone)]
pub enum CmdContent {
    String(String),
    Text(Text<'static>)
}

/// The type of block that can be selected.
/// The boolean from Browser indicates whether current dir is the root directory.
pub enum Block {
    Browser(bool),
    CommandLine(CmdContent, CursorPos)
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

    pub fn into_text(&self) -> Text {
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

    /// Get mutable reference of `CmdContent`.
    /// This function can only be used for String enum.
    pub fn get_mut(&mut self) -> &mut String {
        if let Self::String(_ref) = self {
            _ref
        } else {
            panic!("Unknown error when getting mutable ref of CmdContent!")
        }
    }

    // pub fn append_string<S: AsRef<String>>(&mut self, _str: S) {
    //     match *self {
    //         CmdContent::String(ref mut messages) => {
    //             messages.push_str(&format!("\n{}", _str.as_ref()));
    //         },
    //         CmdContent::Text(ref mut text) => {
    //             text.push_line(Line::raw());
    //         },
    //     }
    // }
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
