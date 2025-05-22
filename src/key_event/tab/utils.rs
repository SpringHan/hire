// Utils

use std::{borrow::Cow, path::PathBuf, rc::Rc};

use anyhow::bail;
use toml_edit::{value, Array};
use ratatui::{style::{Modifier, Stylize}, text::{Line, Span, Text}};

use crate::{
    key_event::{move_cursor, Goto, SwitchCase, SwitchCaseData},
    config::{get_conf_file, get_document, write_document},
    error::{AppResult, ErrorType, NotFoundType},
    app::{path_is_hidden, App},
    utils::CmdContent,
    option_get,
    rt_error,
};

use super::types::TabState;

pub fn quick_switch(app: &mut App, key: char) -> AppResult<()> {
    let mut state = TabState::default();
    state.single_index = true;

    update_current_tab(app);
    handle_tabs(app, key, &mut state)?;
    Ok(())
}

pub fn tab_operation(app: &mut App) -> AppResult<()> {
    update_current_tab(app);

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
                CmdContent::Text(Text::raw(
                    "Are you sure to store current tabs?"
                ).red()),
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

pub fn next(app: &mut App) -> AppResult<bool> {
    if app.tab_list.list.len() == app.tab_list.current + 1 {
        return Ok(true)
    }

    update_current_tab(app);

    let tab = &mut app.tab_list;
    tab.current += 1;
    select_new(app)?;

    Ok(true)
}

pub fn prev(app: &mut App) -> AppResult<bool> {
    if app.tab_list.current == 0 {
        return Ok(true)
    }

    update_current_tab(app);
    let tab = &mut app.tab_list;
    tab.current -= 1;
    select_new(app)?;

    Ok(true)
}

// NOTE: As the new tab is created with current directory, there's no need to call goto function.
#[inline]
fn create(app: &mut App) {
    let tab = &mut app.tab_list;
    tab.list.push((app.path.to_owned(), app.hide_files));
    tab.selected_file.push(None);
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
        tab.selected_file.remove(idx);
        tab.current -= 1;

        // Focus the previous tab.
        select_new(app)?;

        return Ok(true)
    }

    if idx < tab.current {
        tab.current -= 1;
    }
    tab.list.remove(idx);
    tab.selected_file.remove(idx);

    Ok(true)
}

#[inline]
fn delete_other_tabs(app: &mut App) {
    let tab_list = &mut app.tab_list;

    if tab_list.list.len() == 1 {
        return ()
    }

    let tab = tab_list.list[tab_list.current].to_owned();
    let selected_idx = tab_list.selected_file[tab_list.current];

    tab_list.list.clear();
    tab_list.selected_file.clear();

    tab_list.list.push(tab);
    tab_list.selected_file.push(selected_idx);
    tab_list.current = 0;
}

fn handle_tabs(app: &mut App, key: char, data: &mut TabState) -> AppResult<bool> {
    // Handle tabs index
    let tabs_len = if data.storage {
        app.tab_list.storage.len()
    } else {
        app.tab_list.list.len()
    };
    let length_width = tabs_len.to_string().chars().count();

    let idx = if !data.single_index && tabs_len > 9 {
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

        if let Some(idx) = app.tab_list.selected_file[app.tab_list.current] {
            if idx != 0 {
                move_cursor(app, Goto::Index(idx), app.root())?;
            }
        }
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
    app.tab_list.selected_file = vec![None; tabs.len()];
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
        document["storage_tabs"].as_array_mut()
            .unwrap()
            .push(Array::default());
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

/// Update current tab info.
fn update_current_tab(app: &mut App) {
    let selected_idx = if app.root() {
        app.selected_item.parent_selected()
    } else {
        app.selected_item.current_selected()
    };

    let tab = &mut app.tab_list;
    tab.list[tab.current] = (
        app.path.to_owned(),
        app.hide_files
    );
    tab.selected_file[tab.current] = selected_idx;
}

fn select_new(app: &mut App) -> AppResult<()> {
    let target_tab = app.tab_list.list
        .get(app.tab_list.current)
        .expect("Failed when switching tab!")
        .to_owned();
    app.goto_dir(target_tab.0, Some(target_tab.1))?;

    if let Some(idx) = app.tab_list.selected_file[app.tab_list.current] {
        if idx != 0 {
            move_cursor(app, Goto::Index(idx), app.root())?;
        }
    }

    Ok(())
}

fn generate_msg(app: Option<&App>, data: &TabState) -> AppResult<CmdContent> {
    let mut text = Text::raw("[n] create new tab  [f] next tab  [b] prev tab  [c] close current tab
[d] delete tab with number  [s] open tabs from storage  [S] store opening tabs
[o] delete other tabs\n\n");

    if data.delete {
        text.push_line(Line::raw("Executing delete operation!").red());
        text.push_line("");
    }

    if data.storage {
        text.push_line("Storage tabs:");
    }

    if let Some(_app) = app {
        if data.storage {
            storage_tab_string(&mut text, _app.tab_list.storage.iter())?;
        } else {
            tab_string_list(&mut text, _app.tab_list.list.iter(), _app.tab_list.current);
        }
    }

    Ok(CmdContent::Text(text))
}

#[inline]
fn storage_tab_string<'a, I>(text: &mut Text, iter: I) -> AppResult<()>
where I: Iterator<Item = &'a Rc<[Cow<'a, str>]>>
{
    let mut idx = 1;

    for tabs in iter {
        if tabs.is_empty() {
            rt_error!("Found empty tabs from `storage_tabs`")
        }

        text.push_line(format!("[{}]: {}\n", idx, tabs[0]));
        for j in 1..tabs.len() {
            text.push_line(format!("     {}\n", tabs[j]));
        }
        text.push_line("");

        idx += 1;
    }

    Ok(())
}

#[inline]
fn tab_string_list<'a, I>(text: &mut Text, iter: I, current: usize)
where I: Iterator<Item = &'a (PathBuf, bool)>
{
    let mut idx = 1;

    for e in iter {
        let mut line = Line::default();
        line.push_span(
            Span::raw(format!("[{}]", idx))
                .add_modifier(if current + 1 == idx {
                    Modifier::REVERSED
                } else {
                    Modifier::empty()
                })
        );
        line.push_span(format!(": {}", e.0.to_string_lossy()));
        
        text.push_line(line);
        idx += 1;
    }
}
