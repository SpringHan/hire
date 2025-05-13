// Edit mode

mod types;

use std::fs;

use ratatui::{style::{Color, Stylize}, text::Text, widgets::ListState};

use crate::{
    app::App, error::{AppError, AppResult}, option_get, utils::{delete_word, CmdContent, CursorPos, FileContent}
};

use super::{cursor_movement::move_cursor_core, Goto, SwitchCaseData};

pub use types::*;

// NOTE: The Edit mode can only be used for current_block.
// And it's not allow to enable it in root directory of Linux.
impl EditMode {
    pub fn cursor_move(
        &mut self,
        right: bool,
        edge: bool
    ) -> AppResult<()> {
        for item in self.items.iter_mut() {
            if !right {
                match item.cursor {
                    CursorPos::Index(ref mut idx) => {
                        if edge {
                            item.cursor = CursorPos::Index(0);
                        } else {
                            if *idx > 0 {
                                *idx -= 1;
                            }
                        }
                    },
                    CursorPos::End => {
                        if item.editing_name.is_empty() {
                            continue;
                        }

                        if edge {
                            item.cursor = CursorPos::Index(0);
                        } else {
                            item.cursor = CursorPos::Index(
                                item.editing_name.len() - 1
                            );
                        }
                    },

                    CursorPos::None => ()
                }
            } else {
                match item.cursor {
                    CursorPos::Index(ref mut index) => {
                        if edge {
                            item.cursor = CursorPos::End;
                        } else {
                            if *index < item.editing_name.len() - 1 {
                                *index += 1;
                                continue;
                            }

                            if *index == item.editing_name.len() - 1 {
                                item.cursor = CursorPos::End;
                            }
                        }
                    },
                    _ => ()
                }
            }
        }

        Ok(())
    }

    /// Enter insert modal.
    pub fn enter_insert(&mut self, state: &mut ListState, pos: CursorPos) {
        if self.items.is_empty() {
            return ()
        }

        let mut really_insert = false;
        if !self.marked.is_empty() {
            for (idx, item) in self.items.iter_mut().enumerate() {
                if self.marked.contains(&idx) {
                    if !really_insert {
                        really_insert = true;
                    }

                    item.cursor = pos;
                }
            }

            self.marked.clear();
            self.insert = really_insert;
            return ()
        }

        if let Some(selected_item) = state.selected() {
            let item = &mut self.items[selected_item];
            if !self.marked.contains(&selected_item) {
                if !really_insert {
                    really_insert = true;
                }

                item.cursor = pos;
            }
        }

        self.insert = really_insert;
    }

    pub fn insert_char(&mut self, _char: char) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue;
            }

            if let CursorPos::Index(ref mut idx) = item.cursor {
                item.editing_name.insert(*idx, _char);
                *idx += 1;
            } else {
                item.editing_name.push(_char);
            }
        }
    }

    pub fn backspace(&mut self, by_word: bool) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue;
            }

            if item.editing_name.is_empty() {
                continue;
            }

            if by_word {
                delete_word(&mut item.editing_name, &mut item.cursor);
                continue;
            }

            if let CursorPos::Index(ref mut idx) = item.cursor {
                if *idx != 0 {
                    item.editing_name.remove(*idx - 1);
                    *idx -= 1;
                }
            } else {
                item.editing_name.pop();
            }
        }
    }

    pub fn create_item(&mut self, state: &mut ListState, dir: bool) -> AppResult<()> {
        self.items.push(EditItem {
            editing_name: String::new(),
            is_dir: dir,
            delete: false,
            cursor: CursorPos::End
        });

        self.insert = true;
        state.select(Some(self.items.len() - 1));

        Ok(())
    }

    pub fn escape_insert(&mut self, state: &mut ListState, files_length: usize) {
        self.insert = false;

        if self.items.is_empty() {
            return ()
        }

        let mut temp_items: Vec<EditItem> = Vec::new();
        for (idx, item) in self.items.iter_mut().enumerate() {
            if idx >= files_length && item.editing_name.is_empty() {
                continue;
            }

            if item.cursor != CursorPos::None {
                item.cursor = CursorPos::None;
            }

            temp_items.push(item.to_owned());
        }

        self.items = temp_items;
        if let Some(selected_idx) = state.selected() {
            if selected_idx >= self.items.len() {
                state.select(Some(self.items.len() - 1));
            }
        }
    }
}

/// Mark or unmark file(s) as `delete`.
pub fn mark_delete(app: &mut App) -> AppResult<()> {
    let edit_ref = &mut app.edit_mode;
    let state = &mut app.selected_item.current;

    if edit_ref.items.is_empty() {
        return Ok(())
    }

    let err_msg = "Canno find the {i}th item";
    if !edit_ref.marked.is_empty() {
        for i in edit_ref.marked.iter() {
            let item = option_get!(edit_ref.items.get_mut(*i), err_msg);
            item.delete = !item.delete;
        }

        edit_ref.marked.clear();
        return Ok(())
    }

    if let Some(selected_idx) = state.selected() {
        let item = option_get!(edit_ref.items.get_mut(selected_idx), err_msg);
        item.delete = !item.delete;
        item_navigation(app, Goto::Down)?;
    }

    Ok(())
}

pub fn item_navigation(
    app: &mut App,
    direction: Goto,
) -> AppResult<()>
{
    let edit_ref = &mut app.edit_mode;
    let state = &mut app.selected_item.current;

    if edit_ref.items.is_empty() || state.selected().is_none() {
        return Ok(())
    }

    let expand_region = move_cursor_core(
        direction,
        state,
        edit_ref.items.len(),
        app.mark_expand
    );

    if let Some(range) = expand_region {
        for i in range {
            if !edit_ref.marked.contains(&i) {
                edit_ref.marked.push(i);
            }
        }
    }

    if let Some(idx) = app.selected_item.current.selected() {
        if idx < app.current_files.len() {
            app.init_child_files()?;
            app.refresh_child_item();
        } else {
            // BUG: Maybe there'll be a bug.
            app.file_content = FileContent::Text(Text::default());
        }
    }

    Ok(())
}

pub fn mark_operation(app: &mut App, single: bool) -> AppResult<()> {
    let edit_ref = &mut app.edit_mode;

    if single {
        if let Some(idx) = app.selected_item.current.selected() {
            edit_ref.mark_unmark(idx)?;

            item_navigation(app, Goto::Down)?;
        }
    } else {
        if edit_ref.marked.is_empty() {
            edit_ref.marked.extend(0..edit_ref.items.len());
        } else {
            edit_ref.marked.clear();
        }
    }

    Ok(())
}

pub fn save_edit(app: &mut App, key: char, _: SwitchCaseData) -> AppResult<bool> {
    if key != 'y' {
        return Ok(true)
    }

    let edit_ref = &mut app.edit_mode;
    if edit_ref.items.is_empty() {
        return Ok(true)
    }

    let mut have_hide_file = false;
    let mut errors = AppError::new();
    let mut renamed_files: Vec<String> = Vec::new();
    for (idx, item) in edit_ref.items.iter().enumerate() {
        if idx < app.current_files.len() {
            let file = app.current_files.get(idx).unwrap();
            let file_path = app.path.join(&file.name);
            if item.delete && !renamed_files.contains(&file.name) {
                if file.is_dir {
                    if let Err(err) = fs::remove_dir_all(file_path) {
                        errors.add_error(err);
                    }
                } else {
                    if let Err(err) = fs::remove_file(file_path) {
                        errors.add_error(err);
                    }
                }

                continue;
            }

            if item.editing_name != file.name {
                if item.editing_name.starts_with(".") {
                    have_hide_file = true;
                }

                let new_path = app.path.join(&item.editing_name);
                renamed_files.push(item.editing_name.to_owned());

                if let Err(err) = fs::rename(file_path, new_path) {
                    errors.add_error(err);
                }
            }

            continue;
        }

        if item.editing_name.starts_with(".") {
            have_hide_file = true;
        }

        let file_path = app.path.join(&item.editing_name);
        if item.is_dir {
            if let Err(err) = fs::create_dir(file_path) {
                errors.add_error(err);
            }
        } else {
            if let Err(err) = fs::File::create_new(file_path) {
                errors.add_error(err);
            }
        }
    }

    edit_ref.reset();
    app.goto_dir(app.current_path(), Some(!have_hide_file))?;

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(true)
}

pub fn generate_msg() -> CmdContent {
    CmdContent::Text(
        Text::raw("Are you sure to apply the edited files? (y to confirm)")
            .fg(Color::Red)
    )
}
