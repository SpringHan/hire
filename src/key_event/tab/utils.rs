//! The tab menu: switching, creating, removing and storing tabs.

use std::{borrow::Cow, path::PathBuf};

use anyhow::bail;
use toml_edit::{value, Array};
use ratatui::{style::Stylize, text::{Line, Span, Text}};

use crate::{
    key_event::{
        move_cursor, Goto, MenuList, MenuPresentation, SwitchCase, SwitchCaseData,
    },
    config::{get_conf_file, get_document, write_document},
    error::{AppResult, ErrorType, NotFoundType},
    app::{path_is_hidden, App},
    ui::key_hints,
    utils::CmdContent,
    option_get,
    rt_error,
};

use super::types::{Tab, TabAction, TabId, TabState};

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
        MenuPresentation::Picker,
        "Tabs",
        generate_menu(Some(app), &TabState::default())?,
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

    if let Some(action) = data.pending.take() {
        return confirm_action(app, key, action, data)
    }

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

        'S' => {
            data.set_saving();
            SwitchCase::new(
                app,
                switch,
                MenuPresentation::Inline,
                "Tabs",
                CmdContent::Text(Text::raw(
                    "Are you sure to store current tabs?"
                ).red()),
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },

        's' => {
            let menu = generate_menu(Some(app), data.set_storage())?;

            SwitchCase::new(
                app,
                switch,
                MenuPresentation::Picker,
                "Tabs",
                menu,
                SwitchCaseData::Struct(Box::new(data))
            );
            return Ok(false)
        },

        'd' => {
            let menu = generate_menu(
                Some(app),
                data.set_delete()
            )?;

            SwitchCase::new(
                app,
                switch,
                MenuPresentation::Picker,
                "Tabs",
                menu,
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

fn open_tab_menu(app: &mut App, data: &TabState) -> AppResult<()> {
    let menu = generate_menu(Some(app), data)?;

    SwitchCase::new(
        app,
        switch,
        MenuPresentation::Picker,
        "Tabs",
        menu,
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
        MenuPresentation::Inline,
        "Tabs",
        CmdContent::Text(Text::raw(message).red()),
        SwitchCaseData::Struct(Box::new(data.to_owned()))
    );

    Ok(false)
}

fn shells_text(count: usize) -> String {
    if count == 1 {
        String::from("1 shell")
    } else {
        format!("{count} shells")
    }
}

fn shells_pronoun(count: usize) -> &'static str {
    if count == 1 { "it" } else { "them" }
}

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

pub fn next(app: &mut App) -> AppResult<bool> {
    if app.tab_list.len() == app.tab_list.current() + 1 {
        return Ok(true)
    }

    let idx = app.tab_list.current() + 1;
    switch_to(app, idx)?;

    Ok(true)
}

pub fn prev(app: &mut App) -> AppResult<bool> {
    if app.tab_list.current() == 0 {
        return Ok(true)
    }

    let idx = app.tab_list.current() - 1;
    switch_to(app, idx)?;

    Ok(true)
}

// The new tab is created on the current directory, which the browser already
// shows, so there is nothing to goto.
#[inline]
fn create(app: &mut App) {
    let path = app.path.to_owned();
    let hide_files = app.hide_files;

    app.tab_list.push(path, hide_files);

    let idx = app.tab_list.len() - 1;
    app.tab_list.set_current(idx);
}

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
                MenuPresentation::Picker,
                "Tabs",
                generate_menu(Some(app), data)?,
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


    // `delete` and `storage` combine: deleting in storage mode drops the
    // stored group, not an open tab.
    if data.delete {
        if data.storage {
            return Ok(remove_storage_tabs(app, idx - 1)?)
        }

        return request_remove(app, idx - 1, data)
    }

    if data.storage {
        return request_apply_storage(app, idx - 1, data)
    }

    if app.tab_list.len() < idx {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    switch_to(app, idx - 1)?;

    Ok(true)
}


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

fn kill_tab_shells(app: &mut App, idx: usize) {
    let tab_id = app.tab_list.tabs().get(idx).map(|tab| tab.id);

    if let Some(tab_id) = tab_id {
        app.close_tab_shells(tab_id);
    }
}

/// Store the browser's path, hidden-file flag and selection into the current
/// tab.
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

/// Switch to the tab identified by `id`.
///
/// Used by `shell::attach` to show a shell of another tab; the view state of
/// the tab being left is saved and the target's is restored, so the tab list
/// stays consistent.
pub fn switch_to_tab(app: &mut App, id: TabId) -> AppResult<()> {
    let Some(idx) = app
        .tab_list
        .tabs()
        .iter()
        .position(|tab| tab.id == id)
    else {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    };

    switch_to(app, idx)
}

/// Switch to the tab at `idx`; the browser state and the tab list stay in sync.
fn switch_to(app: &mut App, idx: usize) -> AppResult<()> {
    if idx == app.tab_list.current() {
        return Ok(())
    }

    let previous = app.tab_list.current();
    update_current_tab(app);
    app.tab_list.set_current(idx);

    if let Err(err) = select_new(app) {
        // Best effort rollback: never leave the tab list pointing at a tab
        // whose browser state could not be restored.
        app.tab_list.set_current(previous);
        let _ = select_new(app);
        return Err(err)
    }

    Ok(())
}

/// The tab menu: key hints, the active mode and the tab (or stored group)
/// entries.
fn generate_menu(app: Option<&App>, data: &TabState) -> AppResult<MenuList> {
    // The keys are drawn in the marked colour; without an app (the tab list is
    // still being restored) the hints stay uncoloured.
    let keys = app
        .map(|app| app.term_colors.marked_style)
        .unwrap_or_default();

    let mut menu = MenuList::new()
        .header(key_hints(
            "[n] create new tab  [f] next tab  [b] prev tab  [c] close current tab",
            keys
        ))
        .header(key_hints(
            "[d] delete tab with number  [o] delete other tabs",
            keys
        ))
        .header(key_hints(
            "[s] open tabs from storage  [S] store opening tabs",
            keys
        ))
        .header(Line::default());

    if data.delete {
        menu = menu.header(Line::raw("Executing delete operation!").red());
        menu = menu.header(Line::default());
    }

    if data.storage {
        menu = menu.header(Line::raw("Storage tabs:"));
    }

    if let Some(app) = app {
        if data.storage {
            menu = storage_rows(menu, app, data)?;
        } else {
            menu = tab_rows(menu, app, data);
        }
    }

    Ok(menu)
}

/// The `0`-based entry index the typed number names, if it names one.
fn typed_index(data: &TabState, len: usize) -> Option<usize> {
    if data.selecting.is_empty() {
        return None
    }

    let idx = data.calc_idx();

    if idx >= 1 && idx <= len {
        Some(idx - 1)
    } else {
        None
    }
}

/// One entry per open tab, the current one unless a number is being typed.
fn tab_rows(mut menu: MenuList, app: &App, data: &TabState) -> MenuList {
    for (idx, tab) in app.tab_list.tabs().iter().enumerate() {
        let mut row = Line::raw(tab.path.to_string_lossy().into_owned());

        if !tab.shells.is_empty() {
            row.push_span(
                Span::raw(format!("  ({})", shells_text(tab.shells.len()))).dim()
            );
        }

        menu = menu.entry((idx + 1).to_string(), row);
    }

    menu.select(
        typed_index(data, app.tab_list.len())
            .or(Some(app.tab_list.current()))
    )
}

/// One entry per stored group, followed by the group's other paths.
fn storage_rows(
    mut menu: MenuList,
    app: &App,
    data: &TabState
) -> AppResult<MenuList> {
    for (idx, tabs) in app.tab_list.storage.iter().enumerate() {
        if tabs.is_empty() {
            rt_error!("Found empty tabs from `storage_tabs`")
        }

        menu = menu.entry(
            (idx + 1).to_string(),
            Line::raw(tabs[0].to_string())
        );

        for path in tabs.iter().skip(1) {
            menu = menu.line(Line::raw(format!("     {}", path)));
        }

        menu = menu.line(Line::default());
    }

    Ok(menu.select(typed_index(data, app.tab_list.storage.len())))
}

#[cfg(test)]
mod test {
    use std::{borrow::Cow, path::Path};

    use crate::{
        key_event::{
            shell::{GraphicsSupport, ShellOrigin},
            switch::switch_match,
        },
        utils::ShellId,
    };

    use super::*;

    /// Mount a shell session on the tab at `index`, without attaching it.
    fn mount_shell(app: &mut App, index: usize) -> ShellId {
        let tab_id = app.tab_list.tabs()[index].id;
        let id = app
            .shell_sessions
            .open(
                tab_id,
                "/bin/sh",
                Path::new("/"),
                GraphicsSupport::disabled(),
                ShellOrigin::Browser,
            )
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
    fn test_tab_menu_lists_tabs_and_highlights_the_current() {
        let mut app = App::default();
        app.tab_list.push(PathBuf::from("/tmp/second"), false);
        app.tab_list.push(PathBuf::from("/tmp/third"), false);
        app.tab_list.set_current(1);

        let menu = generate_menu(Some(&app), &TabState::default()).unwrap();

        let keys = menu.rows().iter().filter_map(|row| match row {
            crate::key_event::MenuRow::Entry(entry) => Some(entry.key.as_str()),
            crate::key_event::MenuRow::Line(_) => None,
        }).collect::<Vec<_>>();

        assert_eq!(keys, vec!["1", "2", "3"]);
        assert_eq!(menu.selected_row(), Some(1));

        // A typed tab number (only needed past nine tabs) moves it.
        let mut data = TabState::default();
        data.selecting.push(2);

        let menu = generate_menu(Some(&app), &data).unwrap();
        assert_eq!(menu.selected_row(), Some(1));
    }

    #[test]
    fn test_tab_menu_lists_stored_groups() {
        let mut app = App::default();
        app.tab_list.storage.push(
            vec![
                Cow::Owned(String::from("/tmp/a")),
                Cow::Owned(String::from("/tmp/b")),
            ].into()
        );

        let mut data = TabState::default();
        data.set_storage();

        let menu = generate_menu(Some(&app), &data).unwrap();

        assert_eq!(menu.entry_count(), 1);
        // The entry, the continuation line and the blank line after the group.
        assert_eq!(menu.rows().len(), 3);

        data.selecting.push(1);
        let menu = generate_menu(Some(&app), &data).unwrap();
        assert_eq!(menu.selected_row(), Some(0));
    }
}
