// Edit mode

mod types;

use ratatui::{text::Text, widgets::ListState};

use crate::{app::{App, CursorPos, FileContent}, error::AppResult, option_get};

use super::{cursor_movement::move_cursor_core, Goto};

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

    /// Mark or unmark file(s) as `delete`.
    pub fn mark_delete(&mut self, state: &mut ListState) -> AppResult<()> {
        if self.items.is_empty() {
            return Ok(())
        }

        let err_msg = "Canno find the {i}th item";
        if !self.marked.is_empty() {
            for i in self.marked.iter() {
                let item = option_get!(self.items.get_mut(*i), err_msg);
                item.delete = !item.delete;
            }

            self.marked.clear();
            return Ok(())
        }

        if let Some(selected_idx) = state.selected() {
            let item = option_get!(self.items.get_mut(selected_idx), err_msg);
            item.delete = !item.delete;
        }

        Ok(())
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

    pub fn backspace(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue;
            }

            if item.editing_name.is_empty() {
                continue;
            }

            if let CursorPos::Index(ref mut idx) = item.cursor {
                if *idx != 0 {
                    item.editing_name.remove(*idx);
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
            edit_ref.mark_unmark(i)?;
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
