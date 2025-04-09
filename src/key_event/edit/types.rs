// Types

use crate::{app::FileSaver, utils::CursorPos, error::{AppResult, ErrorType}};

#[derive(Clone)]
pub struct EditItem {
    pub(super) editing_name: String,
    /// This attribute is only for items that has no filesaver.
    pub is_dir: bool,

    // pub(super) mark: bool,
    pub(super) delete: bool,
    pub(super) cursor: CursorPos,
}

#[derive(Default)]
pub struct EditMode {
    pub(super) insert: bool,
    pub(crate) enabled: bool,
    pub(super) marked: Vec<usize>,

    pub(super) items: Vec<EditItem>
}

impl EditItem {
    pub fn delete(&self) -> bool {
        self.delete
    }

    pub fn name(&self) -> &str {
        &self.editing_name
    }

    pub fn cursor(&self) -> CursorPos {
        self.cursor
    }
}

impl EditMode {
    pub fn init<'a, I>(&mut self, files: I)
    where I: Iterator<Item = &'a FileSaver>
    {
        if !self.items.is_empty() {
            self.items.clear();
        }

        for file in files {
            self.items.push(EditItem {
                editing_name: file.name.to_owned(),
                is_dir: false,

                // mark: false,
                delete: false,
                cursor: CursorPos::None
            });
        }

        if !self.items.is_empty() {
            self.enabled = true;
        }
    }

    pub fn inserting(&self) -> bool {
        self.insert
    }

    pub fn iter(&self) -> impl Iterator<Item = &EditItem> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_marked(&self, idx: usize) -> bool {
        self.marked.contains(&idx)
    }

    pub fn has_marked(&self) -> bool {
        !self.marked.is_empty()
    }

    pub fn mark_unmark(&mut self, idx: usize) -> AppResult<()> {
        if idx < self.items.len() {
            let index = self.marked.iter().position(|index| *index == idx);
            if let Some(position_idx) = index {
                self.marked.remove(position_idx);
            } else {
                self.marked.push(idx);
            }

            return Ok(())
        }

        Err(ErrorType::NoSelected.pack())
    }

    pub fn reset(&mut self) {
        self.items.clear();
        self.marked.clear();
        self.insert = false;
        self.enabled = false;
    }
}
