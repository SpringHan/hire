//! The recorded keys of a macro and the items it runs on.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyEvent;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MacroStatus {
    Recording,
    Executing,
    None
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MacroTarget {
    pub dir: PathBuf,
    pub name: String,
}

pub struct Macro {
    keys: Vec<KeyEvent>,
    /// The items the macro runs on; empty means the selected item.
    targets: Vec<MacroTarget>,
    pub status: MacroStatus,
}

impl Macro {
    pub fn new() -> Self {
        Macro {
            keys: Vec::new(),
            targets: Vec::new(),
            status: MacroStatus::None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn get_keys(&self) -> Vec<KeyEvent> {
        self.keys.to_owned()
    }

    pub fn record_key(&mut self, key: KeyEvent) {
        self.keys.push(key);
    }

    pub fn clear_keys(&mut self) {
        self.keys.clear();
    }

    pub fn set_targets(&mut self, targets: Vec<MacroTarget>) {
        self.targets = targets;
    }

    pub fn take_targets(&mut self) -> Vec<MacroTarget> {
        std::mem::take(&mut self.targets)
    }
}
