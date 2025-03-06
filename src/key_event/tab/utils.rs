// Utils

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use crate::{
    app::App,
    key_event::{SwitchCase, SwitchCaseData},
    error::{AppResult, ErrorType, NotFoundType},
    rt_error
};

use super::types::TabState;

pub fn tab_operation(app: &mut App) -> AppResult<()> {
    // Update tab status in current tab
    app.tab_list.list[app.tab_list.current] = (
        app.path.to_owned(),
        app.hide_files
    );

    SwitchCase::new(
        app,
        switch,
        generate_msg(Some(app), TabState::default())?,
        SwitchCaseData::Struct(TabState::wrap())
    );

    Ok(())
}

fn switch(app: &mut App, key: char, _data: SwitchCaseData) -> AppResult<bool> {
    let mut data = if let SwitchCaseData::Struct(data) = _data {
        match data.as_any().downcast_ref::<TabState>() {
            Some(case) => case.to_owned(),
            None => panic!("Unknow panic occurred at switch fn in utils.rs!"),
        }
    } else {
        panic!("Unexpected situation at switch funciton in tab.rs.")
    };

    match key {
        'n' => create(app),
        'o' => delete_other_tabs(app),
        'f' => return Ok(next(app)?),
        'b' => return Ok(prev(app)?),
        'c' => return Ok(remove_base(app, app.tab_list.current)?),
        's' => {
            let msg = generate_msg(Some(app), data.set_storage())?;

            SwitchCase::new(
                app,
                switch,
                msg,
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },
        'd' => {
            app.tab_list.list[app.tab_list.current] = (
                app.path.to_owned(),
                app.hide_files
            );

            let msg = generate_msg(
                Some(app),
                data.set_delete()
            )?;

            SwitchCase::new(
                app,
                switch,
                msg,
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },
        // TODO: Extract this into a function
        '1'..='9' => {
            let idx = key
                .to_digit(10)
                .expect("Failed to parse char to usize!") as usize;

            if app.tab_list.list.len() < idx {
                return Err(ErrorType::NotFound(NotFoundType::None).pack())
            }

            // TODO: delete also for storage tabs
            if data.delete {
                return Ok(remove_base(app, idx - 1)?);
            }

            let tab = &mut app.tab_list;
            if let Some(path) = tab.list.get(idx - 1).cloned() {
                tab.current = idx - 1;
                app.goto_dir(path.0, Some(path.1))?;
                return Ok(true)
            }
        },
        _ => ()
    }

    Ok(true)
}

fn next(app: &mut App) -> AppResult<bool> {
    let tab = &mut app.tab_list;
    if tab.list.len() == tab.current + 1 {
        rt_error!("There's no other tabs")
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

    Ok(true)
}

fn prev(app: &mut App) -> AppResult<bool> {
    let tab = &mut app.tab_list;
    if tab.current == 0 {
        rt_error!("There's no other tabs")
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

    Ok(true)
}

// NOTE: As the new tab is created with current directory, there's no need to call goto function.
#[inline]
fn create(app: &mut App) {
    let tab = &mut app.tab_list;
    tab.list[tab.current] = (
        app.path.to_owned(),
        app.hide_files
    );
    tab.list.push((app.path.to_owned(), app.hide_files));
    tab.current = tab.list.len() - 1;
}

// Remove tab with its idx. Return false if failed to remove tab.
fn remove_base(app: &mut App, idx: usize) -> AppResult<bool> {
    let tab = &mut app.tab_list;

    if idx == tab.current {
        if tab.list.len() == 1 {
            rt_error!("There's only one tab")
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

        return Ok(true)
    }

    if tab.current != 0 {
        tab.current -= 1;
    }
    tab.list.remove(idx);

    Ok(true)
}

#[inline]
fn delete_other_tabs(app: &mut App) {
    let tab_list = &mut app.tab_list;

    if tab_list.list.len() == 1 {
        return ()
    }

    let tab = tab_list.list.get(tab_list.current)
        .expect("Error code 1 at delete_other_tabs in tab.rs!")
        .to_owned();

    tab_list.list.clear();
    tab_list.list.push(tab);
    tab_list.current = 0;
}

fn generate_msg(app: Option<&App>, data: TabState) -> AppResult<String> {
    let mut msg = if let Some(_app) = app {
        if data.storage {
            storage_tab_string(_app.tab_list.storage.iter())?
        } else {
            tab_string_list(_app.tab_list.list.iter())
        }
    } else {
        String::new()
    };

    if data.storage {
        msg.insert_str(0, "Storage tabs:\n");
    }

    if data.delete {
        msg.insert_str(0, "Executing delete operation!\n\n");
    }

    msg.insert_str(0, "[n] create new tab  [f] next tab  [b] prev tab  [c] close current tab
[d] delete tab with number  [s] open tabs from storage  [o] delete other tabs\n\n");

    Ok(msg)
}

#[inline]
fn storage_tab_string<'a, I>(iter: I) -> AppResult<String>
where I: Iterator<Item = &'a Rc<[Cow<'a, str>]>>
{
    let mut msg = String::new();
    let mut idx = 1;

    for tabs in iter {
        if tabs.is_empty() {
            rt_error!("Found empty tabs from `storage_tabs`")
        }

        msg.push_str(&format!("[{}]: {}\n", idx, tabs[0]));
        for j in 1..tabs.len() {
            msg.push_str(&format!("     {}\n", tabs[j]));
        }
        msg.push('\n');

        idx += 1;
    }

    Ok(msg)
}

#[inline]
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
