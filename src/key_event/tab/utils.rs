// Utils

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use anyhow::bail;
use toml_edit::{value, Array};

use crate::{
    config::{get_conf_file, get_document, write_document},
    error::{AppResult, ErrorType, NotFoundType},
    key_event::{SwitchCase, SwitchCaseData},
    app::{path_is_hidden, App},
    option_get,
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
        true,
        generate_msg(Some(app), &TabState::default())?,
        SwitchCaseData::Struct(TabState::wrap())
    );

    Ok(())
}

// Core util functions
fn switch(app: &mut App, key: char, _data: SwitchCaseData) -> AppResult<bool> {
    let mut data = if let SwitchCaseData::Struct(data) = _data {
        match data.as_any().downcast_ref::<TabState>() {
            Some(case) => case.to_owned(),
            None => panic!("Unknow panic occurred at switch fn in utils.rs!"),
        }
    } else {
        panic!("Unexpected situation at switch funciton in tab.rs.")
    };

    // Trying to save opening tabs
    if data.save_tabs {
        if key == 'y' {
            save_tabs(app)?;
        }

        return Ok(true)
    }

    match key {
        'n'       => create(app),
        'f'       => return Ok(next(app)?),
        'b'       => return Ok(prev(app)?),
        'o'       => delete_other_tabs(app),
        '0'..='9' => return Ok(handle_tabs(app, key, &mut data)?),
        'c'       => return Ok(remove_base(app, app.tab_list.current)?),

        'S' => {
            data.set_saving();
            SwitchCase::new(
                app,
                switch,
                false,
                String::from("Are you sure to store current tabs?"),
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },

        's' => {
            let msg = generate_msg(Some(app), data.set_storage())?;

            SwitchCase::new(
                app,
                switch,
                true,
                msg,
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },

        'd' => {
            // TODO: Useless code
            // app.tab_list.list[app.tab_list.current] = (
            //     app.path.to_owned(),
            //     app.hide_files
            // );

            let msg = generate_msg(
                Some(app),
                data.set_delete()
            )?;

            SwitchCase::new(
                app,
                switch,
                true,
                msg,
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
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

#[inline]
fn handle_tabs(app: &mut App, key: char, data: &mut TabState) -> AppResult<bool> {
    // Handle tabs index
    let tabs_len = if data.storage {
        app.tab_list.storage.len()
    } else {
        app.tab_list.list.len()
    };
    let length_width = tabs_len.to_string().chars().count();

    let idx = if tabs_len > 9 {
        let idx = key.to_digit(10)
            .expect("Failed to parse char to usize!") as u8;
        data.selecting.push(idx);

        if data.selecting.len() < length_width {
            SwitchCase::new(
                app,
                switch,
                true,
                generate_msg(Some(app), data)?,
                SwitchCaseData::Struct(Box::new(data.to_owned()))
            );

            return Ok(false)
        }

        data.calc_idx()
    } else {
        key.to_digit(10)
            .expect("Failed to parse char to usize!") as usize
    };


    // Delete specific tab or storage tabs
    if data.delete {
        if data.storage {
            return Ok(remove_storage_tabs(app, idx - 1)?)
        }

        return Ok(remove_base(app, idx - 1)?)
    }

    // Apply storage tabs
    if data.storage {
        return Ok(apply_storage_tabs(app, idx - 1)?)
    }

    // Switch specific tab
    if app.tab_list.list.len() < idx {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let tab = &mut app.tab_list;
    if let Some(path) = tab.list.get(idx - 1).cloned() {
        tab.current = idx - 1;
        app.goto_dir(path.0, Some(path.1))?;
        return Ok(true)
    }

    Ok(true)
}


// Non-core functions
fn apply_storage_tabs(app: &mut App, idx: usize) -> AppResult<bool> {
    if idx >= app.tab_list.storage.len() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let mut tabs: Vec<(PathBuf, bool)> = Vec::new();
    for path_str in app.tab_list.storage[idx].iter() {
        let path = PathBuf::from(path_str.as_ref());
        let is_hidden = path_is_hidden(&path);
        tabs.push((path, is_hidden));
    }

    if tabs.is_empty() {
        rt_error!("The selected storaage tabs array is empty")
    }

    app.tab_list.current = 0;
    app.tab_list.list = tabs;

    let first = app.tab_list.list[0].to_owned();
    app.goto_dir(first.0, Some(!first.1))?;
    
    Ok(true)
}

fn save_tabs(app: &mut App) -> anyhow::Result<()> {
    let type_err = "The value type of `storage_tabs` is error";

    let tabs = app.tab_list.list.to_owned();
    let mut document = get_document(get_conf_file()?.0)?;
    let _array = if let Some(value) = document.get_mut("storage_tabs") {
        let temp = option_get!(value.as_array_mut(), type_err);
        temp.push(Array::default());
        temp.get_mut(temp.len() - 1)
            .unwrap()
            .as_array_mut()
            .unwrap()
    } else {
        document["storage_tabs"] = value(Array::default());
        document["storage_tabs"][0] = value(Array::default());
        document["storage_tabs"][0]
            .as_array_mut()
            .unwrap()
    };

    let mut fmt_tabs: Vec<Cow<str>> = Vec::new();
    for (path, _) in tabs.into_iter() {
        let tab_path = if let Ok(_path) = path.into_os_string().into_string() {
            _path
        } else {
            bail!("Failed to convert PathBuf to String when saving tabs")
        };

        _array.push(&tab_path);
        fmt_tabs.push(Cow::Owned(tab_path));
    }

    write_document(document)?;
    app.tab_list.storage.push(fmt_tabs.into());

    Ok(())
}

fn remove_storage_tabs(app: &mut App, idx: usize) -> anyhow::Result<bool> {
    let type_err = "The value type of `storage_tabs` is error";
    let non_err = "The item you want to remove doesn't exist";

    let mut document = get_document(get_conf_file()?.0)?;
    if let Some(value) = document.get_mut("storage_tabs") {
        let _array = option_get!(value.as_array_mut(), type_err);
        if _array.len() <= idx {
            bail!("{}", non_err)
        }

        _array.remove(idx);
    } else {
        bail!("{}", non_err)
    }

    write_document(document)?;
    if app.tab_list.storage.len() <= idx {
        bail!("{}", non_err)
    }

    app.tab_list.storage.remove(idx);

    Ok(true)
}

fn generate_msg(app: Option<&App>, data: &TabState) -> AppResult<String> {
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
[d] delete tab with number  [s] open tabs from storage  [S] store opening tabs
[o] delete other tabs\n\n");

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
