// Types

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use crate::{
    key_event::switch::SwitchStruct,
    utils::ShellId,
};

/// The stable identity of a tab.
///
/// Tab positions change when a tab before the current one is removed, but the
/// shells mounted on a tab must keep pointing at their owner, so tabs are
/// identified by this value instead of by their index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TabId(pub u64);

/// One open tab: its directory, its remembered selection and the shells
/// mounted on it.
#[derive(Clone)]
pub struct Tab {
    pub id: TabId,
    pub path: PathBuf,
    pub hide_files: bool,
    pub selected: Option<usize>,

    /// The shells mounted on this tab, in the order they were opened.
    pub shells: Vec<ShellId>,

    /// The shell which was attached last, used when re-entering the tab.
    pub active_shell: Option<ShellId>,
}

impl Tab {
    pub fn new(id: TabId, path: PathBuf, hide_files: bool) -> Self {
        Self {
            id,
            path,
            hide_files,
            selected: None,
            shells: Vec::new(),
            active_shell: None,
        }
    }
}

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
    pub(super) list: Vec<Tab>,

    /// A collection of specific tabs stored in auto_config.toml
    pub(super) storage: Vec<Rc<[Cow<'a, str>]>>,

    /// The next id handed out to a new tab.
    pub(super) next_id: u64,
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
            list: vec![Tab::new(TabId(0), path, false)],
            storage: Vec::new(),
            current: 0,
            next_id: 1,
        }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// All open tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.list
    }

    pub fn current_tab(&self) -> &Tab {
        // NOTE: `current` is kept inside `0..len` by every mutating method.
        &self.list[self.current]
    }

    pub fn current_tab_mut(&mut self) -> &mut Tab {
        &mut self.list[self.current]
    }

    pub fn tab_by_id(&self, id: TabId) -> Option<&Tab> {
        self.list.iter().find(|tab| tab.id == id)
    }

    pub fn tab_by_id_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.list.iter_mut().find(|tab| tab.id == id)
    }

    /// Hand out a fresh tab id (used when a caller builds `Tab`s itself).
    pub fn next_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Append a new tab and return its id.
    pub fn push(&mut self, path: PathBuf, hide_files: bool) -> TabId {
        let id = self.next_tab_id();
        self.list.push(Tab::new(id, path, hide_files));
        id
    }

    /// Replace the whole tab list (used when restoring a session or a stored
    /// tab group). Provided ids are kept so mounted shells stay associated.
    pub fn set_tabs(&mut self, tabs: Vec<Tab>) {
        if tabs.is_empty() {
            return
        }

        for tab in &tabs {
            if tab.id.0 >= self.next_id {
                self.next_id = tab.id.0 + 1;
            }
        }

        self.list = tabs;
        self.current = self.current.min(self.list.len() - 1);
    }

    pub fn set_current(&mut self, idx: usize) {
        self.current = idx.min(self.list.len().saturating_sub(1));
    }

    /// Remove the tab at `idx`; the current index is kept in range and moves
    /// to the previous tab when the removed one was focused.
    pub fn remove(&mut self, idx: usize) -> Option<Tab> {
        if idx >= self.list.len() || self.list.len() == 1 {
            return None
        }

        let removed = self.list.remove(idx);

        if idx < self.current {
            self.current -= 1;
        } else if idx == self.current {
            self.current = self.current.saturating_sub(1);
        }

        Some(removed)
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

    #[test]
    fn test_tab_ids_are_stable_across_removal() {
        let mut tabs = TabList::new(PathBuf::from("/tmp"));
        let first = tabs.push(PathBuf::from("/a"), false);
        let second = tabs.push(PathBuf::from("/b"), false);
        let third = tabs.push(PathBuf::from("/c"), false);
        tabs.set_current(3);

        tabs.remove(2);

        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs.current(), 2);
        assert_eq!(tabs.current_tab().id, third);
        assert!(tabs.tab_by_id(first).is_some());
        assert!(tabs.tab_by_id(second).is_none());
        assert!(tabs.tab_by_id(third).is_some());
    }
}
