// Tab.

use super::SwitchCase;
use crate::app::{self, App};
use crate::app::command::OperationError;

use std::io;
use std::error::Error;
use std::path::PathBuf;

pub struct TabList {
    list: Vec<(PathBuf, bool)>, // Store current path & whether hiding files.
    current: usize
}

impl TabList {
    pub fn new(path: PathBuf) -> Self {
        TabList {
            list: vec![(path, false)],
            current: 0
        }
    }
}

pub fn tab_operation(app: &mut App) -> Result<(), Box<dyn Error>> {
    // Update tab status in current tab
    app.tab_list.list[app.tab_list.current] = (
        app.path.to_owned(),
        app.hide_files
    );

    SwitchCase::new(app, switch, generate_msg(app), Some(false));

    Ok(())
}

fn next(app: &mut App) -> io::Result<()> {
    let tab = &mut app.tab_list;
    if tab.list.len() == tab.current + 1 {
        OperationError::Specific("There's no other tabs!".to_owned()).check(app);
        return Ok(())
    }

    tab.list[tab.current] = (
        app.path.to_owned(),
        app.hide_files
    );
    tab.current += 1;

    let target_tab = tab.list
        .get(tab.current)
        .expect("Unable to get next tab!")
        .to_owned();
    app.goto_dir(target_tab.0, Some(target_tab.1))?;

    Ok(())
}

fn prev(app: &mut App) -> io::Result<()> {
    let tab = &mut app.tab_list;
    if tab.current == 0 {
        OperationError::Specific("There's no other tabs!".to_owned()).check(app);
        return Ok(())
    }

    tab.list[tab.current] = (
        app.path.to_owned(),
        app.hide_files
    );
    tab.current -= 1;

    let target_tab = tab.list
        .get(tab.current)
        .expect("Unable to get prev tab!")
        .to_owned();
    app.goto_dir(target_tab.0, Some(target_tab.1))?;

    Ok(())
}

// NOTE: As the new tab is created with current directory, there's no need to call goto function.
fn create(app: &mut App) {
    let tab = &mut app.tab_list;
    tab.list[tab.current] = (
        app.path.to_owned(),
        app.hide_files
    );
    tab.list.push((app.path.to_owned(), app.hide_files));
    tab.current += 1;
}

fn remove_base(app: &mut App, idx: usize) -> io::Result<()> {
    let tab = &mut app.tab_list;

    if idx == tab.current {
        if tab.list.len() == 1 {
            OperationError::Specific("There's only one tab!".to_owned()).check(app);
            return Ok(())
        }
        tab.list.remove(idx);

        // Focus the previous tab.
        if idx != 0 {
            tab.current -= 1;
        }
        let target_tab = tab.list
            .get(tab.current)
            .expect("Failed to switch to nearby tabs!")
            .to_owned();
        app.goto_dir(target_tab.0, Some(target_tab.1))?;

        return Ok(())
    }

    if tab.current != 0 {
        tab.current -= 1;
    }
    tab.list.remove(idx);

    Ok(())
}

fn remove_export(app: &mut App, idx: char) -> Result<(), Box<dyn Error>> {
    let idx = idx
        .to_digit(10)
        .expect("Failed to parse char to usize!") as usize;
    remove_base(app, idx - 1)?;

    Ok(())
}

fn switch(app: &mut App, key: char, to_delete: Option<bool>) -> Result<bool, Box<dyn Error>> {
    if to_delete.is_none() {
        panic!("Unexpected situation at switch funciton in tab.rs.")
    }

    let to_delete = to_delete.unwrap();
    match key {
        'n' => create(app),
        'f' => next(app)?,
        'b' => prev(app)?,
        'c' => remove_base(app, app.tab_list.current)?,
        'd' => {
            let mut msg = generate_msg(app);
            msg.insert_str(0, "Deleting tab!\n");

            app.tab_list.list[app.tab_list.current] = (
                app.path.to_owned(),
                app.hide_files
            );
            // let msg = tab_string_list(app.tab_list.list.iter());
            // SwitchCase::new(app, remove_export, msg);
            return Ok(true)
        },
        '1'..='9' => {
            let idx = key
                .to_digit(10)
                .expect("Failed to parse char to usize!") as usize;
            let tab = &mut app.tab_list;
            if let Some(path) = tab.list.get(idx - 1).cloned() {
                tab.current = idx - 1;
                app.goto_dir(path.0, Some(path.1))?;
                return Ok(true)
            }

            OperationError::NotFound(None).check(app);
        },
        _ => ()
    }

    Ok(true)
}

fn generate_msg(app: &App) -> String {
    let mut msg = tab_string_list(app.tab_list.list.iter());
    msg.insert_str(0, "[n] create new tab  [f] next tab  [b] prev tab  [c] close current tab
[d] delete tab with number");

    msg
}


fn tab_string_list<'a, I>(iter: I) -> String
where I: Iterator<Item = &'a (PathBuf, bool)>
{
    let mut msg = String::new();
    let mut idx = 1;

    for e in iter {
        msg.push_str(&format!("[{}]: {}\n", idx, e.0.to_string_lossy()));
        idx += 1;
    }

    msg
}


#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        assert_eq!('1'.to_digit(10).unwrap(), 1);
    }
}
