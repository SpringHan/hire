// Types

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use crate::key_event::switch::SwitchStruct;

#[derive(Default, Clone, Copy)]
pub struct TabState {
    pub(super) delete: bool,
    pub(super) saving: bool,
    pub(super) storage: bool
}

pub struct TabList<'a> {
    pub(super) current: usize,

    /// Store current path & whether hiding files.
    pub(super) list: Vec<(PathBuf, bool)>,

    /// A collection of specific tabs stored in auto_config.toml
    pub(super) storage: Vec<Rc<[Cow<'a, str>]>>,
}

impl SwitchStruct for TabState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TabState {
    pub fn wrap() -> Box<Self> {
        Box::new(Self::default())
    }

    pub fn set_delete(&mut self) -> Self {
        self.delete = true;
        *self
    }
    
    pub fn set_storage(&mut self) -> Self {
        self.storage = true;
        *self
    }
    
    pub fn set_saving(&mut self) -> Self {
        self.saving = true;
        *self
    }
}

impl<'a> TabList<'a> {
    pub fn new(path: PathBuf) -> Self {
        TabList {
            list: vec![(path, false)],
            storage: Vec::new(),
            current: 0
        }
    }
}
