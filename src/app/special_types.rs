// Special types used in App structure.

use std::collections::HashMap;

use ratatui::widgets::ListState;

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

/// The type of block that can be selected.
/// The boolean from Browser indicates whether current dir is the root directory.
pub enum Block {
    Browser(bool),
    CommandLine(String, CursorPos)
}

/// The Move operation includes the move of file & the creation of symbolic link of file.
#[derive(Clone, Copy, PartialEq)]
pub enum FileOperation {
    Move,
    None
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
    Text(String),
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
