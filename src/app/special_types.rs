// Special types used in App structure.

use std::collections::HashSet;

use ratatui::widgets::ListState;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CursorPos {
    Index(usize),
    End
}

pub enum OptionFor {
    Goto,
    Delete,
    None
}

/// The type of block that can be selected.
/// The boolean from Browser indicates whether current dir is the root directory.
pub enum Block {
    Browser(bool),
    CommandLine(String, CursorPos)
}

#[derive(Clone)]
pub enum FileOperation {
    Delete,
    Move,
    None
}

#[derive(Clone)]
pub struct MarkedFiles {
    pub files: HashSet<String>,
    pub operation: FileOperation
}

impl Default for MarkedFiles {
    fn default() -> Self {
        MarkedFiles {
            files: HashSet::new(),
            operation: FileOperation::None
        }
    }
}

pub struct ItemIndex {
    pub parent: ListState,
    pub current: ListState,
    pub child: ListState
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
