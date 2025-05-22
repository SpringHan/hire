// Types

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use crate::key_event::switch::SwitchStruct;

#[derive(Clone)]
pub struct TabState {
    pub(super) delete: bool,
    pub(super) storage: bool,

    /// Whether the user attempts to save opening tabs.
    pub(super) save_tabs: bool,

    /// Whether just receive a single character of tab index.
    pub(super) single_index: bool,

    /// The cache of selecting number of tabs.
    pub(super) selecting: Vec<u8>,
}

pub struct TabList<'a> {
    pub(super) current: usize,

    /// The selected file index in every tab.
    pub(super) selected_file: Vec<Option<usize>>,

    /// Store current path & whether hiding files.
    pub(super) list: Vec<(PathBuf, bool)>,

    /// A collection of specific tabs stored in auto_config.toml
    pub(super) storage: Vec<Rc<[Cow<'a, str>]>>,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            delete: false,
            storage: false,
            save_tabs: false,
            single_index: false,
            selecting: Vec::new(),
        }
    }
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

    pub fn set_delete(&mut self) -> &mut Self {
        self.delete = true;
        self
    }
    
    pub fn set_storage(&mut self) -> &mut Self {
        self.storage = true;
        self
    }
    
    pub fn set_saving(&mut self) -> &mut Self {
        self.save_tabs = true;
        self
    }

    pub fn calc_idx(&self) -> usize {
        let mut idx = 0;
        let mut pow = 0;

        for i in self.selecting.iter().rev() {
            idx += 10u8.pow(pow) * i;
            pow += 1;
        }

        idx as usize
    }
}

impl<'a> TabList<'a> {
    pub fn new(path: PathBuf) -> Self {
        TabList {
            list: vec![(path, false)],
            selected_file: vec![None],
            storage: Vec::new(),
            current: 0,
        }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
}

#[cfg(test)]
mod tests {
    use crate::app::path_is_hidden;

    use super::*;

    #[test]
    fn test1() {
        let mut state = TabState::default();
        state.selecting.push(2);
        state.selecting.push(0);
        state.selecting.push(2);
        println!("Result: {}", state.calc_idx());
    }

    #[test]
    fn test_hidden() {
        println!("{}", path_is_hidden("/home/spring"));
    }
}
