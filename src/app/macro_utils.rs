// Macro utils

use std::path::PathBuf;

use ratatui::crossterm::event::KeyEvent;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MacroStatus {
    Recording,
    Executing,
    None
}

/// The item that a macro should be executed on.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MacroTarget {
    /// The directory which contains the target item.
    pub dir: PathBuf,
    /// The name of the target item.
    pub name: String,
}

pub struct Macro {
    keys: Vec<KeyEvent>,
    /// The items that the macro should be executed on.
    /// When it's empty, the macro is executed on the selected item.
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

    /// Set the items that the macro should be executed on.
    pub fn set_targets(&mut self, targets: Vec<MacroTarget>) {
        self.targets = targets;
    }

    /// Take the items that the macro should be executed on.
    pub fn take_targets(&mut self) -> Vec<MacroTarget> {
        std::mem::take(&mut self.targets)
    }
}
