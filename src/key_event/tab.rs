// Tab.

use super::SwitchCase;
use crate::app::App;
use crate::app::command::OperationError;

use std::io;
use std::path::PathBuf;
use std::ops::{AddAssign, SubAssign};

pub struct TabList {
    list: Vec<PathBuf>,
    current: usize
}

impl Default for TabList {
    fn default() -> Self {
        TabList {
            list: Vec::new(),
            current: 0
        }
    }
}

fn next(app: &mut App) -> io::Result<()> {
    let tab = &mut app.tab_list;
    if tab.list.len() == tab.current + 1 {
        OperationError::Specific("There's no other tabs!".to_owned()).check(app);
        return Ok(())
    }

    tab.list[tab.current] = app.path.to_owned();
    tab.current.add_assign(1);

    let target_tab = tab.list
        .get(tab.current)
        .expect("Unable to get next tab!")
        .to_owned();
    app.goto_dir(target_tab)?;

    Ok(())
}

fn prev(app: &mut App) -> io::Result<()> {
    let tab = &mut app.tab_list;
    if tab.current == 0 {
        OperationError::Specific("There's no other tabs!".to_owned()).check(app);
        return Ok(())
    }

    tab.list[tab.current] = app.path.to_owned();
    tab.current.sub_assign(1);

    let target_tab = tab.list
        .get(tab.current)
        .expect("Unable to get prev tab!")
        .to_owned();
    app.goto_dir(target_tab)?;

    Ok(())
}

// NOTE: As the new tab is created with current directory, there's no need to call goto function.
fn insert(app: &mut App) {
    let tab = &mut app.tab_list;
    tab.list[tab.current] = app.path.to_owned();
    tab.list.push(app.path.to_owned());
    tab.current.add_assign(1);
}

fn remove(app: &mut App, idx: usize) -> io::Result<()> {
    let tab = &mut app.tab_list;
    if idx == tab.current {
        if tab.list.len() == 1 {
            OperationError::Specific("There's only one tab!".to_owned()).check(app);
            return Ok(())
        }
        tab.list.remove(idx);

        // Focus the previous tab.
        if idx != 0 {
            tab.current.sub_assign(1);
        }
        let target_tab = tab.list
            .get(tab.current)
            .expect("Failed to switch to nearby tabs!")
            .to_owned();
        app.goto_dir(target_tab)?;

        return Ok(())
    }

    tab.list.remove(idx);

    Ok(())
}

fn switch(app: &mut App, idx: usize) -> io::Result<()> {
    Ok(())
}

pub fn tab_operation(app: &mut App, key: char) -> io::Result<()> {
    Ok(())
}
