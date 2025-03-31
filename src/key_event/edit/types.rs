// Types

use std::rc::Rc;

use crate::{app::{App, CursorPos, FileData}, error::{AppResult, ErrorType}};

pub struct EditItem {
    pub(super) editing_name: String,
    pub reference: Option<Rc<FileData>>,

    pub(super) mark: bool,
    pub(super) delete: bool,
    pub(super) cursor: CursorPos,
}

#[derive(Default)]
pub struct EditMode {
    pub(crate) enabled: bool,
    pub(super) insert: bool,

    pub(super) items: Vec<EditItem>
}

impl EditItem {
    pub fn marked(&self) -> bool {
        self.mark
    }

    pub fn name(&self) -> &str {
        &self.editing_name
    }
}

impl EditMode {
    pub fn init<'a, I>(&mut self, files: I)
    where I: Iterator<Item = &'a FileSaver>
    {
        if !self.items.is_empty() {
            self.items.clear();
        }

        // for file in files {
        //     self.items.push(EditItem {
        //         editing_name: file.name.to_owned(),
        //         reference: Some(file),

        //         mark: false,
        //         delete: false,
        //         cursor: CursorPos::None
        //     });
        // }
    }

    pub fn inserting(&self) -> bool {
        self.insert
    }

    pub fn iter(&'a self) -> impl Iterator<Item = &'a EditItem> {
        self.items.iter()
    }

    pub fn mark_item(&mut self, idx: usize) -> AppResult<()> {
        if let Some(item) = self.items.get_mut(idx) {
            item.mark = true;
            return Ok(())
        }

        Err(ErrorType::NoSelected.pack())
    }
}
