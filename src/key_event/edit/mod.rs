// Edit mode

mod types;

use std::fs;

use ratatui::{style::{Color, Stylize}, text::Text, widgets::ListState};

use crate::{
    app::App, error::{AppError, AppResult}, option_get, utils::{
        cursor_from_insertion_point,
        cursor_insertion_point,
        delete_word,
        next_char_boundary,
        prev_char_boundary,
        CmdContent,
        CursorPos,
        FileContent,
    }
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
            if item.cursor == CursorPos::None {
                continue
            }

            if edge {
                item.cursor = if right {
                    CursorPos::End
                } else {
                    CursorPos::Index(0)
                };

                continue
            }

            // The cursor is a byte offset on a character boundary, so the
            // movement has to jump over the whole character (e.g. a CJK one).
            let Some(pos) = cursor_insertion_point(
                &item.editing_name,
                item.cursor
            ) else {
                continue
            };

            item.cursor = if right {
                match next_char_boundary(&item.editing_name, pos) {
                    Some(next) => cursor_from_insertion_point(
                        &item.editing_name,
                        next
                    ),
                    None => CursorPos::End,
                }
            } else {
                match prev_char_boundary(&item.editing_name, pos) {
                    Some(previous) => CursorPos::Index(previous),
                    None => CursorPos::Index(0),
                }
            };
        }

        Ok(())
    }

    /// Move the cursor of the editing items to the end of the nearest word
    /// (`Alt-f`).
    ///
    /// Like [`Self::cursor_move`], every item whose cursor is active is moved,
    /// which handles the marked items as well.
    pub fn cursor_word_forward(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue
            }

            crate::utils::cursor_next_word_end(
                &item.editing_name,
                &mut item.cursor
            );
        }
    }

    /// Move the cursor of the editing items to the start of the nearest word
    /// (`Alt-b`).
    pub fn cursor_word_backward(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue
            }

            crate::utils::cursor_prev_word_start(
                &item.editing_name,
                &mut item.cursor
            );
        }
    }

    /// Delete the whole word before the cursor of the editing items
    /// (`Ctrl-Backspace`).
    pub fn delete_word_before_cursor(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue
            }

            crate::utils::delete_word_before_cursor(
                &mut item.editing_name,
                &mut item.cursor
            );
        }
    }

    /// Delete the whole word after the cursor of the editing items (`Alt-d`).
    pub fn delete_word_after_cursor(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue
            }

            crate::utils::delete_word_after_cursor(
                &mut item.editing_name,
                &mut item.cursor
            );
        }
    }

    /// Delete the character after the cursor of the editing items (`Ctrl-d`).
    pub fn delete_char_after_cursor(&mut self) {
        for item in self.items.iter_mut() {
            if item.cursor == CursorPos::None {
                continue
            }

            crate::utils::delete_char_after_cursor(
                &mut item.editing_name,
                &mut item.cursor
            );
        }
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
            let Some(pos) = cursor_insertion_point(
                &item.editing_name,
                item.cursor
            ) else {
                continue
            };

            item.editing_name.insert(pos, _char);

            // The cursor is a byte offset, so it has to move over the whole
            // inserted character, which may take several bytes (e.g. CJK).
            item.cursor = cursor_from_insertion_point(
                &item.editing_name,
                pos + _char.len_utf8()
            );
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

            let Some(pos) = cursor_insertion_point(
                &item.editing_name,
                item.cursor
            ) else {
                continue
            };

            let Some(start) = prev_char_boundary(&item.editing_name, pos)
            else {
                continue
            };

            // Remove the whole preceding character, which may take several
            // bytes (e.g. CJK).
            item.editing_name.replace_range(start..pos, "");
            item.cursor = cursor_from_insertion_point(
                &item.editing_name,
                start
            );
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
    mut direction: Goto,
) -> AppResult<()>
{
    if let Goto::Index(ref mut idx) = direction {
        let file_length = if app.root() {
            app.parent_files.len()
        } else {
            app.current_files.len()
        };

        if *idx >= file_length {
            *idx = file_length - 1;
        }
    }

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

#[cfg(test)]
mod test {
    use super::*;

    fn edit_mode_with(name: &str, cursor: CursorPos) -> EditMode {
        EditMode {
            items: vec![EditItem {
                editing_name: String::from(name),
                is_dir: false,
                delete: false,
                cursor,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_cursor_word_movement() {
        let mut edit_mode = edit_mode_with("oldname.txt", CursorPos::End);

        edit_mode.cursor_word_backward();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(8));

        edit_mode.cursor_word_backward();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        edit_mode.cursor_word_forward();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(7));

        edit_mode.cursor_word_forward();
        assert!(edit_mode.items[0].cursor == CursorPos::End);

        assert_eq!(edit_mode.items[0].editing_name, "oldname.txt");
    }

    #[test]
    fn test_delete_word_before_cursor() {
        let mut edit_mode = edit_mode_with("oldname.txt", CursorPos::End);

        edit_mode.delete_word_before_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "oldname.");
        assert!(edit_mode.items[0].cursor == CursorPos::End);

        // The separators between the word and the cursor are removed as well.
        edit_mode.delete_word_before_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "");
        assert!(edit_mode.items[0].cursor == CursorPos::End);

        // A word in the middle of the name is deleted from its start.
        let mut edit_mode = edit_mode_with("hello world", CursorPos::Index(8));
        edit_mode.delete_word_before_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "hello rld");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(6));
    }

    #[test]
    fn test_word_operations_only_touch_active_items() {
        let mut edit_mode = EditMode {
            items: vec![
                EditItem {
                    editing_name: String::from("one two"),
                    is_dir: false,
                    delete: false,
                    cursor: CursorPos::End,
                },
                EditItem {
                    editing_name: String::from("abc"),
                    is_dir: false,
                    delete: false,
                    cursor: CursorPos::None,
                },
            ],
            ..Default::default()
        };

        edit_mode.delete_word_before_cursor();
        edit_mode.cursor_word_backward();

        assert_eq!(edit_mode.items[0].editing_name, "one ");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        // The item without a cursor is untouched.
        assert_eq!(edit_mode.items[1].editing_name, "abc");
        assert!(edit_mode.items[1].cursor == CursorPos::None);
    }

    #[test]
    fn test_delete_char_after_cursor() {
        let mut edit_mode = edit_mode_with("abc.txt", CursorPos::Index(0));

        edit_mode.delete_char_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "bc.txt");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        // Repeated deletions remove the following characters one by one.
        edit_mode.delete_char_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "c.txt");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        // Nothing to delete at the end of the name.
        let mut edit_mode = edit_mode_with("abc.txt", CursorPos::End);
        edit_mode.delete_char_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "abc.txt");
        assert!(edit_mode.items[0].cursor == CursorPos::End);
    }

    #[test]
    fn test_delete_word_after_cursor() {
        // Delete the word right after the cursor.
        let mut edit_mode = edit_mode_with("hello.txt", CursorPos::Index(0));

        edit_mode.delete_word_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, ".txt");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        // Nothing to delete at the end of the name.
        let mut edit_mode = edit_mode_with("hello.txt", CursorPos::End);

        edit_mode.delete_word_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "hello.txt");
        assert!(edit_mode.items[0].cursor == CursorPos::End);

        // A word in the middle of the name is deleted up to its end.
        let mut edit_mode = edit_mode_with("hello world", CursorPos::Index(2));

        edit_mode.delete_word_after_cursor();
        assert_eq!(edit_mode.items[0].editing_name, "he world");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(2));
    }

    #[test]
    fn test_insert_char_with_multibyte_content() {
        let mut edit_mode = edit_mode_with("中b", CursorPos::Index(3));

        edit_mode.insert_char('X');
        assert_eq!(edit_mode.items[0].editing_name, "中Xb");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(4));

        // The cursor moves over the whole inserted character.
        edit_mode.insert_char('文');
        assert_eq!(edit_mode.items[0].editing_name, "中X文b");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(7));
    }

    #[test]
    fn test_cursor_move_with_multibyte_content() {
        let mut edit_mode = edit_mode_with("中文ab", CursorPos::End);

        edit_mode.cursor_move(false, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(7));

        edit_mode.cursor_move(false, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(6));

        edit_mode.cursor_move(false, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(3));

        edit_mode.cursor_move(false, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        edit_mode.cursor_move(true, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(3));

        edit_mode.cursor_move(true, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(6));

        edit_mode.cursor_move(true, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(7));

        edit_mode.cursor_move(true, false).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::End);

        // The edge movement still jumps to the beginning and the end.
        edit_mode.cursor_move(false, true).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        edit_mode.cursor_move(true, true).unwrap();
        assert!(edit_mode.items[0].cursor == CursorPos::End);
    }

    #[test]
    fn test_backspace_with_multibyte_content() {
        // The whole preceding character is deleted.
        let mut edit_mode = edit_mode_with("中a", CursorPos::Index(3));

        edit_mode.backspace(false);
        assert_eq!(edit_mode.items[0].editing_name, "a");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));

        let mut edit_mode = edit_mode_with("中a", CursorPos::End);

        edit_mode.backspace(false);
        assert_eq!(edit_mode.items[0].editing_name, "中");
        assert!(edit_mode.items[0].cursor == CursorPos::End);
    }

    #[test]
    fn test_delete_word_with_multibyte_content() {
        let mut edit_mode = edit_mode_with("中文 x", CursorPos::Index(6));

        edit_mode.delete_word_before_cursor();
        assert_eq!(edit_mode.items[0].editing_name, " x");
        assert!(edit_mode.items[0].cursor == CursorPos::Index(0));
    }
}
