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

use super::types::{Tab, TabAction, TabState};

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

    // A destructive action is waiting for the user's confirmation.
    if let Some(action) = data.pending.take() {
        return confirm_action(app, key, action, data)
    }

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
        'o'       => return request_remove_others(app, &mut data),
        '0'..='9' => return Ok(handle_tabs(app, key, &mut data)?),
        'c'       => return request_remove(app, app.tab_list.current(), &mut data),

        // Kill every shell mounted on the current tab; the tab stays open.
        'D' => return request_clear_shells(app, &mut data),

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


/// Run a destructive action the user just confirmed with `y`.
fn confirm_action(
    app: &mut App,
    key: char,
    action: TabAction,
    mut data: TabState,
) -> AppResult<bool>
{
    let confirmed = matches!(key, 'y' | 'Y');

    match action {
        // Clearing the shells keeps the tab, so the menu comes back with the
        // updated shell counts.
        TabAction::ClearShells(tab_id) => {
            if confirmed {
                app.close_tab_shells(tab_id);
            }

            data.selecting.clear();
            open_tab_menu(app, &data)?;

            return Ok(false)
        },

        TabAction::Remove(index) => {
            if confirmed {
                return remove_base(app, index)
            }
        },

        TabAction::RemoveOthers => {
            if confirmed {
                delete_other_tabs(app);
                return Ok(true)
            }
        },

        TabAction::ApplyStorage(index) => {
            if confirmed {
                return apply_storage_tabs(app, index)
            }
        },
    }

    // Cancelled: keep the tab menu (and the mode it was in), drop the partially
    // typed number.
    data.selecting.clear();
    open_tab_menu(app, &data)?;

    Ok(false)
}

/// Show the tab menu again with `data`.
fn open_tab_menu(app: &mut App, data: &TabState) -> AppResult<()> {
    let msg = generate_msg(Some(app), data)?;

    SwitchCase::new(
        app,
        switch,
        true,
        msg,
        SwitchCaseData::Struct(Box::new(data.to_owned()))
    );

    Ok(())
}

/// Show a confirmation page; `false` keeps it open until a key is pressed.
fn request_confirm(
    app: &mut App,
    data: &mut TabState,
    action: TabAction,
    message: String,
) -> AppResult<bool>
{
    data.pending = Some(action);
    data.selecting.clear();

    SwitchCase::new(
        app,
        switch,
        false,
        CmdContent::Text(Text::raw(message).red()),
        SwitchCaseData::Struct(Box::new(data.to_owned()))
    );

    Ok(false)
}

/// "1 shell" / "3 shells".
fn shells_text(count: usize) -> String {
    if count == 1 {
        String::from("1 shell")
    } else {
        format!("{count} shells")
    }
}

/// "it" for one shell, "them" for several.
fn shells_pronoun(count: usize) -> &'static str {
    if count == 1 { "it" } else { "them" }
}

/// Close the tab at `index`, asking first when it has mounted shells.
fn request_remove(app: &mut App, index: usize, data: &mut TabState) -> AppResult<bool> {
    if index >= app.tab_list.len() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    if app.tab_list.len() == 1 {
        rt_error!("There's only one tab")
    }

    let shells = app.tab_list.tabs()[index].shells.len();

    if shells == 0 {
        return remove_base(app, index)
    }

    let message = format!(
        "Tab [{}] has {}; closing the tab will kill {}. Are you sure? (y for yes)",
        index + 1,
        shells_text(shells),
        shells_pronoun(shells)
    );

    request_confirm(app, data, TabAction::Remove(index), message)
}

/// Close every other tab, asking first when they have mounted shells.
fn request_remove_others(app: &mut App, data: &mut TabState) -> AppResult<bool> {
    if app.tab_list.len() == 1 {
        return Ok(true)
    }

    let current = app.tab_list.current();
    let shells = app.tab_list.tabs().iter().enumerate()
        .filter(|(index, _)| *index != current)
        .map(|(_, tab)| tab.shells.len())
        .sum::<usize>();

    if shells == 0 {
        delete_other_tabs(app);
        return Ok(true)
    }

    let message = format!(
        "The other tabs have {}; closing them will kill {}. Are you sure? (y for yes)",
        shells_text(shells),
        shells_pronoun(shells)
    );

    request_confirm(app, data, TabAction::RemoveOthers, message)
}

/// Restore a stored tab group, asking first when the tabs it replaces have
/// mounted shells.
fn request_apply_storage(app: &mut App, index: usize, data: &mut TabState) -> AppResult<bool> {
    if index >= app.tab_list.storage.len() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let shells = app.tab_list.tabs().iter()
        .map(|tab| tab.shells.len())
        .sum::<usize>();

    if shells == 0 {
        return apply_storage_tabs(app, index)
    }

    let tabs = app.tab_list.len();

    let message = format!(
        "Restoring the stored tabs [{}] closes {} and kills {}. Are you sure? (y for yes)",
        index + 1,
        if tabs == 1 {
            String::from("1 open tab")
        } else {
            format!("{tabs} open tabs")
        },
        shells_text(shells)
    );

    request_confirm(app, data, TabAction::ApplyStorage(index), message)
}

/// Kill every shell mounted on the current tab, asking first.
fn request_clear_shells(app: &mut App, data: &mut TabState) -> AppResult<bool> {
    let index = app.tab_list.current();
    let tab = app.tab_list.current_tab();

    if tab.shells.is_empty() {
        // Nothing to kill; the menu stays as it is.
        return Ok(false)
    }

    let tab_id = tab.id;
    let shells = tab.shells.len();

    let message = format!(
        "Tab [{}] has {}; clearing the shells will kill {}. Are you sure? (y for yes)",
        index + 1,
        shells_text(shells),
        shells_pronoun(shells)
    );

    request_confirm(app, data, TabAction::ClearShells(tab_id), message)
}

pub fn next(app: &mut App) -> AppResult<bool> {
    if app.tab_list.len() == app.tab_list.current() + 1 {
        return Ok(true)
    }

    update_current_tab(app);

    let tab = &mut app.tab_list;
    tab.current += 1;
    select_new(app)?;

    Ok(true)
}

pub fn prev(app: &mut App) -> AppResult<bool> {
    if app.tab_list.current() == 0 {
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
    let path = app.path.to_owned();
    let hide_files = app.hide_files;

    app.tab_list.push(path, hide_files);

    let idx = app.tab_list.len() - 1;
    app.tab_list.set_current(idx);
}

// Remove tab with its idx. Return false if failed to remove tab.
fn remove_base(app: &mut App, idx: usize) -> AppResult<bool> {
    if idx >= app.tab_list.len() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    if app.tab_list.len() == 1 {
        rt_error!("There's only one tab")
    }

    let was_current = idx == app.tab_list.current();

    kill_tab_shells(app, idx);

    if app.tab_list.remove(idx).is_none() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    if was_current {
        select_new(app)?;
    }

    Ok(true)
}

#[inline]
fn delete_other_tabs(app: &mut App) {
    if app.tab_list.len() == 1 {
        return ()
    }

    let keep = app.tab_list.current_tab().clone();

    // The removed tabs lose their shells; the kept tab keeps its own.
    let removed = app.tab_list.tabs().iter()
        .filter(|tab| tab.id != keep.id)
        .map(|tab| tab.id)
        .collect::<Vec<_>>();

    for tab_id in removed {
        app.close_tab_shells(tab_id);
    }

    app.tab_list.set_tabs(vec![keep]);
    app.tab_list.set_current(0);
}

fn handle_tabs(app: &mut App, key: char, data: &mut TabState) -> AppResult<bool> {
    // Handle tabs index
    let tabs_len = if data.storage {
        app.tab_list.storage.len()
    } else {
        app.tab_list.len()
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

    // Tab numbers are 1-based; `0` cannot name a tab and would underflow when
    // the item is removed.
    if idx == 0 {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }


    // Delete specific tab or storage tabs
    if data.delete {
        if data.storage {
            return Ok(remove_storage_tabs(app, idx - 1)?)
        }

        return request_remove(app, idx - 1, data)
    }

    // Apply storage tabs
    if data.storage {
        return request_apply_storage(app, idx - 1, data)
    }

    // Switch specific tab
    if app.tab_list.len() < idx {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let target = idx - 1;

    if let Some(tab) = app.tab_list.tabs().get(target).cloned() {
        app.tab_list.set_current(target);
        app.goto_dir(tab.path, Some(tab.hide_files))?;

        if let Some(selected) = tab.selected {
            if selected != 0 {
                move_cursor(app, Goto::Index(selected), app.root())?;
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

    // The tabs which are being replaced lose their shells. `close_tab_shells`
    // also detaches the attached shell, so the view never points at a session
    // which is being dropped.
    let tab_ids = app.tab_list.tabs().iter()
        .map(|tab| tab.id)
        .collect::<Vec<_>>();

    for tab_id in tab_ids {
        app.close_tab_shells(tab_id);
    }

    let mut new_tabs = Vec::new();
    for (path, hide_files) in tabs {
        let id = app.tab_list.next_tab_id();
        new_tabs.push(Tab::new(id, path, hide_files));
    }

    app.tab_list.set_tabs(new_tabs);
    app.tab_list.set_current(0);

    let first = app.tab_list.current_tab().clone();
    app.goto_dir(first.path, Some(!first.hide_files))?;

    Ok(true)
}

fn save_tabs(app: &mut App) -> anyhow::Result<()> {
    let type_err = "The value type of `storage_tabs` is error";

    let tabs = app.tab_list.tabs().to_owned();
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
    for tab in tabs.into_iter() {
        let tab_path = if let Ok(_path) = tab.path.into_os_string().into_string() {
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

/// Close every shell mounted on the tab at `idx`.
fn kill_tab_shells(app: &mut App, idx: usize) {
    let tab_id = app.tab_list.tabs().get(idx).map(|tab| tab.id);

    if let Some(tab_id) = tab_id {
        app.close_tab_shells(tab_id);
    }
}

/// Update current tab info.
pub fn update_current_tab(app: &mut App) {
    let selected_idx = if app.root() {
        app.selected_item.parent_selected()
    } else {
        app.selected_item.current_selected()
    };

    let path = app.path.to_owned();
    let hide_files = app.hide_files;

    let tab = app.tab_list.current_tab_mut();
    tab.path = path;
    tab.hide_files = hide_files;
    tab.selected = selected_idx;
}

fn select_new(app: &mut App) -> AppResult<()> {
    let target = app.tab_list.current_tab().clone();

    app.goto_dir(target.path, Some(target.hide_files))?;

    if let Some(selected) = target.selected {
        if selected != 0 {
            move_cursor(app, Goto::Index(selected), app.root())?;
        }
    }

    Ok(())
}

fn generate_msg(app: Option<&App>, data: &TabState) -> AppResult<CmdContent> {
    let mut text = Text::raw("[n] create new tab  [f] next tab  [b] prev tab  [c] close current tab
[d] delete tab with number  [D] clear shells of current tab  [o] delete other tabs
[s] open tabs from storage  [S] store opening tabs\n\n");

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
            tab_string_list(&mut text, _app.tab_list.tabs().iter(), _app.tab_list.current());
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
where I: Iterator<Item = &'a Tab>
{
    let mut idx = 1;

    for tab in iter {
        let mut line = Line::default();
        line.push_span(
            Span::raw(format!("[{}]", idx))
                .add_modifier(if current + 1 == idx {
                    Modifier::REVERSED
                } else {
                    Modifier::empty()
                })
        );
        line.push_span(format!(": {}", tab.path.to_string_lossy()));

        if !tab.shells.is_empty() {
            line.push_span(
                Span::raw(format!("  ({})", shells_text(tab.shells.len()))).dim()
            );
        }

        text.push_line(line);
        idx += 1;
    }
}

#[cfg(test)]
mod test {
    use std::{borrow::Cow, path::Path};

    use crate::{
        key_event::{shell::GraphicsSupport, switch::switch_match},
        utils::ShellId,
    };

    use super::*;

    /// Mount a shell session on the tab at `index`, without attaching it.
    fn mount_shell(app: &mut App, index: usize) -> ShellId {
        let tab_id = app.tab_list.tabs()[index].id;
        let id = app
            .shell_sessions
            .open(tab_id, "/bin/sh", Path::new("/"), GraphicsSupport::disabled())
            .unwrap();

        let tab = app.tab_list.tab_by_id_mut(tab_id).unwrap();
        tab.shells.push(id);
        tab.active_shell = Some(id);

        id
    }

    /// Open the tab menu and press `keys` one after another.
    fn menu(app: &mut App, keys: &[char]) {
        tab_operation(app).unwrap();

        for key in keys {
            // The dispatcher clones the active case and leaves it stored while
            // the handler returns `false`.
            let case = app.switch_case.clone().expect("the tab menu is open");
            switch_match(app, case, *key).unwrap();
        }
    }

    #[test]
    fn test_close_current_tab_asks_when_shells_are_mounted() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);
        let current = app.tab_list.current();
        mount_shell(&mut app, current);

        menu(&mut app, &['c']);

        // The confirmation is open; the tab and its shell are still there.
        assert!(app.switch_case.is_some());
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 1);

        let case = app.switch_case.take().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        assert_eq!(app.tab_list.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 0);
    }

    #[test]
    fn test_close_current_tab_without_shells_does_not_ask() {
        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);

        menu(&mut app, &['c']);

        // The tab is closed right away; the menu is gone with it.
        assert!(app.switch_case.is_none());
        assert_eq!(app.tab_list.len(), 1);
    }

    #[test]
    fn test_close_current_tab_cancel_keeps_everything() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);
        let current = app.tab_list.current();
        mount_shell(&mut app, current);

        menu(&mut app, &['c', 'n']);

        // Cancelling brings the menu back, with the tab and its shell intact.
        assert!(app.switch_case.is_some());
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 1);
    }

    #[test]
    fn test_delete_tab_by_number_asks_for_mounted_shells() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);
        mount_shell(&mut app, 1);

        menu(&mut app, &['d', '2']);

        assert!(app.switch_case.is_some());
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 1);

        let case = app.switch_case.take().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        assert_eq!(app.tab_list.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 0);
    }

    #[test]
    fn test_close_other_tabs_asks_when_they_have_shells() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);
        mount_shell(&mut app, 1);

        menu(&mut app, &['o']);

        assert!(app.switch_case.is_some());
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 1);

        let case = app.switch_case.take().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        assert_eq!(app.tab_list.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 0);
    }

    #[test]
    fn test_apply_stored_tabs_asks_when_shells_are_mounted() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp"), false);
        mount_shell(&mut app, 0);
        app.tab_list.storage.push(
            vec![Cow::Owned(String::from("/tmp"))].into()
        );

        menu(&mut app, &['s', '1']);

        assert!(app.switch_case.is_some());
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.shell_sessions.alive_count(), 1);

        let case = app.switch_case.take().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        assert_eq!(app.tab_list.len(), 1);
        assert_eq!(app.shell_sessions.alive_count(), 0);
    }

    #[test]
    fn test_clear_shells_keeps_the_tab() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut app = App::default();
        let id = mount_shell(&mut app, 0);

        menu(&mut app, &['D']);

        assert!(app.switch_case.is_some());
        assert_eq!(app.shell_sessions.alive_count(), 1);

        let case = app.switch_case.take().unwrap();
        switch_match(&mut app, case, 'y').unwrap();

        // The tab survives, its shell is gone and the menu is shown again.
        assert_eq!(app.tab_list.len(), 1);
        assert!(app.tab_list.current_tab().shells.is_empty());
        assert!(!app.shell_sessions.is_alive(id));
        assert!(app.switch_case.is_some());
    }

    #[test]
    fn test_clear_shells_without_shells_does_not_ask() {
        let mut app = App::default();

        menu(&mut app, &['D']);

        assert!(app.switch_case.is_some());
        assert!(app.tab_list.current_tab().shells.is_empty());
    }
}
