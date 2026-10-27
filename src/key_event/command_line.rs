// Operations on command line.

use std::borrow::Cow;

use ratatui::widgets::ListState;

use crate::{
    utils::{
        cursor_from_insertion_point,
        cursor_insertion_point,
        display_width,
        next_char_boundary,
        prev_char_boundary,
        Block,
        CmdContent,
        CursorPos,
    },
    error::AppResult,
    utils::str_split,
    option_get,
    app::App,
};

#[derive(PartialEq, Eq)]
enum CompletionType {
    File,
    Builtin
}

#[derive(Default)]
pub struct AppCompletion<'a> {
    max_width: u16,
    show_frame: bool,
    origin_length: u16,
    selected_item: ListState,
    candidates: Vec<Cow<'a, str>>,
}

impl<'a> AppCompletion<'a> {
    pub fn show_frame(&self) -> bool {
        self.show_frame
    }

    pub fn popup_position(&self) -> (u16, u16) {
        (self.origin_length, self.max_width)
    }

    pub fn popup_info(&mut self) -> (
        &Vec<Cow<'a, str>>,
        &mut ListState
    )
    {
        (&self.candidates, &mut self.selected_item)
    }

    pub fn hide(&mut self) {
        self.show_frame = false;
    }

    pub fn reset(&mut self) {
        self.max_width = 0;
        self.origin_length = 0;
        self.show_frame = false;
        self.candidates.clear();
        self.selected_item.select(None);
    }
}

pub fn completion(app: &mut App) -> AppResult<()> {
    // When the cursor is placed right after a space, complete the name of
    // the item selected in the file browser before the cursor.
    if complete_selected_item(app) {
        return Ok(())
    }

    if !app.command_completion.candidates.is_empty() {
        app.command_completion.show_frame = true;
        return Ok(())
    }

    let content_info = get_content(&app.selected_block);
    if content_info.is_none() {
        return Ok(())
    }

    let (content, _) = content_info.unwrap();
    let command_slice: Vec<&str> = content.split(" ").collect();
    let mut position = 0;

    let updated = match command_slice.len() {
        1 => {
            if !command_slice[0].starts_with(":!") &&
                command_slice[0].starts_with(":")
            {
                position = 1;
                update_completion(
                    app,
                    CompletionType::Builtin,
                    &command_slice[0][1..]
                )?
            } else {
                false
            }
        },

        _ => {
            let mut slice = None;
            for i in 0..command_slice.len() {
                if i == command_slice.len() - 1 {
                    if command_slice[i].starts_with("./") {
                        slice = Some(&command_slice[i][2..]);
                        position += 2;
                    } else {
                        slice = Some(command_slice[i]);
                    }

                    break;
                }

                position += display_width(command_slice[i]) + 1;
            }
            update_completion(
                app,
                CompletionType::File,
                slice.unwrap()
            )?
        }
    };

    if updated {
        update_cmdline(
            &mut app.selected_block,
            &mut app.command_completion,
        )?;
        app.command_completion.origin_length = position as u16;
    }

    Ok(())
}

pub fn switch_to(app: &mut App, next: bool) -> AppResult<()> {
    let completion = &mut app.command_completion;
    if completion.candidates.is_empty() {
        return Ok(())
    }

    if let Some(idx) = completion.selected_item.selected_mut() {
        if (*idx == completion.candidates.len() - 1 && next) ||
            (*idx == 0 && !next)
        {
            return Ok(())
        }

        *idx = if next {
            *idx + 1
        } else {
            *idx - 1
        };

        update_cmdline(&mut app.selected_block, completion)?;
    }

    Ok(())
}

pub fn update_cmdline(
    app_block: &mut Block,
    completion: &mut AppCompletion
) -> anyhow::Result<()>
{
    if completion.candidates.is_empty() {
        return Ok(())
    }

    let content_info = get_content(app_block);
    if content_info.is_none() {
        completion.reset();
        return Ok(())
    }


    // Can make completion
    let err_msg = "Cannot get any candidate for completion";

    let (content, _) = content_info.unwrap();
    let mut command_slice = str_split(content);

    let new_value = option_get!(
        completion.candidates.get(option_get!(
            completion.selected_item.selected(),
            err_msg
        )),
        err_msg
    );

    if command_slice.len() == 1 {
        command_slice[0] = format!(":{}", new_value);
    } else {
        let slice = command_slice.last_mut().unwrap();
        *slice = if slice.starts_with("./") {
            format!("./{}", new_value.as_ref())
        } else {
            new_value.as_ref().into()
        };
    }

    let updated_content = command_slice.join(" ");

    // The completion always places the cursor at the end of the updated
    // content, which is represented by `CursorPos::End`.
    app_block.set_command_line(updated_content, CursorPos::End);

    Ok(())
}

pub fn get_content(app_block: &Block) -> Option<(String, CursorPos)> {
    let (_content, cursor) = if let Block::CommandLine(
        ref cont,
        pos
    ) = *app_block {
        (cont.get().to_owned(), pos)
    } else {
        return None
    };

    if _content.is_empty() {
        return None
    }

    // The slice ends after the character at the cursor, which may be a
    // multi-byte one, so the next character boundary is used instead of the
    // byte offset itself.
    let end = match cursor {
        CursorPos::Index(_) => {
            let idx = cursor_insertion_point(&_content, cursor)?;
            next_char_boundary(&_content, idx).unwrap_or(_content.len())
        },
        CursorPos::End => _content.len(),
        CursorPos::None => return None,
    };

    Some((_content[..end].to_owned(), cursor))
}

impl<'a> App<'a> {
    pub fn cursor_left(&mut self, edge: bool) {
        if let Block::CommandLine(
            CmdContent::String(ref command),
            ref mut cursor
        ) = self.selected_block
        {
            let previous = if edge {
                Some(CursorPos::Index(0))
            } else {
                let Some(pos) = cursor_insertion_point(command, *cursor) else {
                    return
                };

                prev_char_boundary(command, pos).map(CursorPos::Index)
            };

            let Some(previous) = previous else {
                return
            };

            if *cursor != previous {
                *cursor = previous;
            }

            if self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    pub fn cursor_right(&mut self, edge: bool) {
        if let Block::CommandLine(
            CmdContent::String(ref command),
            ref mut cursor
        ) = self.selected_block
        {
            let next = if edge {
                Some(CursorPos::End)
            } else {
                let Some(pos) = cursor_insertion_point(command, *cursor) else {
                    return
                };

                Some(
                    match next_char_boundary(command, pos) {
                        Some(next) => cursor_from_insertion_point(command, next),
                        None => CursorPos::End,
                    }
                )
            };

            let Some(next) = next else {
                return
            };

            if *cursor != next {
                *cursor = next;
            }

            if self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    /// Move the cursor to the end of the nearest word at or after the cursor
    /// (`Alt-f`).
    pub fn cursor_word_forward(&mut self) {
        if let Block::CommandLine(
            CmdContent::String(ref content),
            ref mut cursor
        ) = self.selected_block
        {
            crate::utils::cursor_next_word_end(content, cursor);

            if self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    /// Move the cursor to the start of the nearest word before the cursor
    /// (`Alt-b`).
    pub fn cursor_word_backward(&mut self) {
        if let Block::CommandLine(
            CmdContent::String(ref content),
            ref mut cursor
        ) = self.selected_block
        {
            crate::utils::cursor_prev_word_start(content, cursor);

            if self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    /// Delete the whole word before the cursor (`Ctrl-Backspace`).
    pub fn delete_word_before_cursor(&mut self) {
        if let Block::CommandLine(
            CmdContent::String(ref mut content),
            ref mut cursor
        ) = self.selected_block
        {
            let length = content.len();
            crate::utils::delete_word_before_cursor(content, cursor);

            // The candidates are meaningless after the content has changed.
            if content.len() != length && self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    /// Delete the whole word after the cursor (`Alt-d`).
    pub fn delete_word_after_cursor(&mut self) {
        if let Block::CommandLine(
            CmdContent::String(ref mut content),
            ref mut cursor
        ) = self.selected_block
        {
            let length = content.len();
            crate::utils::delete_word_after_cursor(content, cursor);

            // The candidates are meaningless after the content has changed.
            if content.len() != length && self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    /// Delete the character after the cursor (`Ctrl-d`).
    pub fn delete_char_after_cursor(&mut self) {
        if let Block::CommandLine(
            CmdContent::String(ref mut content),
            ref mut cursor
        ) = self.selected_block
        {
            let length = content.len();
            crate::utils::delete_char_after_cursor(content, cursor);

            // The candidates are meaningless after the content has changed.
            if content.len() != length && self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }
}

/// Complete the name of the item selected in the file browser into the
/// command line when the cursor is placed right after a space.
///
/// Return true if the name has been inserted.
fn complete_selected_item(app: &mut App) -> bool {
    if !command_line_after_space(&app.selected_block) {
        return false
    }

    let selected_name = match app.get_file_saver() {
        Some(file) => file.name.to_owned(),
        None => return false,
    };

    // The previous completion is meaningless after inserting a new name.
    app.command_completion.reset();

    insert_before_cursor(&mut app.selected_block, &selected_name)
}

/// Check whether the cursor of the command line is placed right after a space.
///
/// The cursor represented by `CursorPos::Index` points at the character that
/// should be kept on the right side of the insertion point, so the character
/// before the insertion point is located at `idx - 1`.
fn command_line_after_space(app_block: &Block) -> bool {
    let (content, cursor) = if let Block::CommandLine(
        CmdContent::String(ref content),
        cursor
    ) = *app_block {
        (content, cursor)
    } else {
        return false
    };

    match cursor {
        CursorPos::Index(idx) => {
            // `idx` is an index into `content`, which may equal its length
            // when the cursor is at the end of the content.
            idx <= content.len() &&
                idx > 0 &&
                content.as_bytes()[idx - 1] == b' '
        },
        CursorPos::End => content.as_bytes().last() == Some(&b' '),
        CursorPos::None => false,
    }
}

/// Insert `text` right before the cursor and move the cursor to the end of
/// the inserted text.
///
/// Return true if the insertion is done.
fn insert_before_cursor(app_block: &mut Block, text: &str) -> bool {
    if let Block::CommandLine(
        CmdContent::String(ref mut content),
        ref mut cursor
    ) = *app_block {
        let Some(pos) = cursor_insertion_point(content, *cursor) else {
            return false
        };

        content.insert_str(pos, text);

        // The cursor always ends up after the inserted text, and it's switched
        // to `End` when the insertion reaches the end of the content.
        *cursor = cursor_from_insertion_point(content, pos + text.len());

        return true
    }

    false
}

/// Return true if the completion candidates is updated.
fn update_completion(
    app: &mut App,
    _type: CompletionType,
    current: &str
) -> anyhow::Result<bool>
{
    let mut max_width = 0;
    let mut candidates: Vec<Cow<str>> = Vec::new();

    if _type == CompletionType::File {
        let files_iter = if app.root() {
            app.parent_files.iter()
        } else {
            app.current_files.iter()
        };
        
        for file in files_iter {
            if file.name.starts_with(current) {
                let width = display_width(&file.name);
                if max_width < width {
                    max_width = width;
                }
                candidates.push(Cow::Owned(file.name.to_owned()));
            }
        }
    } else {
        let commands = ["rename", "create_file", "create_dir", "create_symlink"];

        for cmd in commands.into_iter() {
            if cmd.starts_with(current) {
                let width = display_width(cmd);
                if max_width < width {
                    max_width = width;
                }
                candidates.push(Cow::Borrowed(cmd));
            }
        }
    }

    if candidates.is_empty() {
        return Ok(false)
    }

    candidates.sort_by(|a, b| a.len().cmp(&b.len()));

    let completion = &mut app.command_completion;
    completion.show_frame = true;
    completion.candidates = candidates;
    completion.max_width = max_width as u16;
    completion.selected_item.select(Some(0));

    Ok(true)
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::app::FileSaver;

    fn app_with(content: &str, cursor: CursorPos) -> App<'static> {
        let mut app = App::default();
        app.selected_block.set_command_line(content, cursor);
        app
    }

    fn command_line<'app, 'a>(app: &'app App<'a>) -> (&'app String, CursorPos) {
        if let Block::CommandLine(
            CmdContent::String(ref content),
            cursor
        ) = app.selected_block
        {
            (content, cursor)
        } else {
            panic!("The command line is not editable")
        }
    }

    /// Describe a cursor position in a readable way, as `CursorPos` doesn't
    /// implement `Debug`.
    fn describe(cursor: CursorPos) -> String {
        match cursor {
            CursorPos::Index(idx) => format!("Index({idx})"),
            CursorPos::End => String::from("End"),
            CursorPos::None => String::from("None"),
        }
    }

    fn assert_command_line(app: &App, content: &str, cursor: CursorPos) {
        let (actual_content, actual_cursor) = command_line(app);
        assert_eq!(actual_content, content);
        assert_eq!(describe(actual_cursor), describe(cursor));
    }

    #[test]
    fn test_cursor_word_forward() {
        let mut app = app_with(":rename foo.txt", CursorPos::Index(0));

        app.cursor_word_forward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(7));

        app.cursor_word_forward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(11));

        app.cursor_word_forward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::End);

        // The cursor stays at the end of the command line.
        app.cursor_word_forward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::End);
    }

    #[test]
    fn test_cursor_word_backward() {
        let mut app = app_with(":rename foo.txt", CursorPos::End);

        app.cursor_word_backward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(12));

        app.cursor_word_backward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(8));

        app.cursor_word_backward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(1));

        app.cursor_word_backward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(0));

        // The cursor stays at the beginning of the command line.
        app.cursor_word_backward();
        assert_command_line(&app, ":rename foo.txt", CursorPos::Index(0));
    }

    #[test]
    fn test_delete_word_before_cursor() {
        // Delete the word from the end of the command line.
        let mut app = app_with(":rename foo.txt", CursorPos::End);
        app.delete_word_before_cursor();
        assert_command_line(&app, ":rename foo.", CursorPos::End);

        // Delete the word in the middle of the command line.
        let mut app = app_with("hello world", CursorPos::Index(8));
        app.delete_word_before_cursor();
        assert_command_line(&app, "hello rld", CursorPos::Index(6));

        // Delete the separators before a word.
        let mut app = app_with("  hello", CursorPos::Index(2));
        app.delete_word_before_cursor();
        assert_command_line(&app, "hello", CursorPos::Index(0));

        // Nothing to delete at the beginning of the command line.
        let mut app = app_with(":rename", CursorPos::Index(0));
        app.delete_word_before_cursor();
        assert_command_line(&app, ":rename", CursorPos::Index(0));

        // Delete the punctuation right before the cursor.
        let mut app = app_with(":rename", CursorPos::Index(1));
        app.delete_word_before_cursor();
        assert_command_line(&app, "rename", CursorPos::Index(0));

        // The word after the command name is deleted with the separators.
        let mut app = app_with(":rename", CursorPos::End);
        app.delete_word_before_cursor();
        assert_command_line(&app, ":", CursorPos::End);

        // The whole content is deleted when the cursor is at its end.
        let mut app = app_with("hello", CursorPos::Index(5));
        app.delete_word_before_cursor();
        assert_command_line(&app, "", CursorPos::End);

        // The completion popup is reset after the deletion.
        let mut app = app_with("hello", CursorPos::End);
        app.command_completion.show_frame = true;
        app.delete_word_before_cursor();
        assert!(!app.command_completion.show_frame());
    }

    #[test]
    fn test_delete_word_after_cursor() {
        // Delete the word right after the cursor.
        let mut app = app_with(":rename foo.txt", CursorPos::Index(1));
        app.delete_word_after_cursor();
        assert_command_line(&app, ": foo.txt", CursorPos::Index(1));

        // The separators between the cursor and the word are deleted as well.
        let mut app = app_with(":rename foo.txt", CursorPos::Index(7));
        app.delete_word_after_cursor();
        assert_command_line(&app, ":rename.txt", CursorPos::Index(7));

        // Nothing to delete at the end of the command line.
        let mut app = app_with(":rename", CursorPos::End);
        app.delete_word_after_cursor();
        assert_command_line(&app, ":rename", CursorPos::End);

        // A multi-byte word is deleted as a whole.
        let mut app = app_with("文件 abc", CursorPos::Index(0));
        app.delete_word_after_cursor();
        assert_command_line(&app, " abc", CursorPos::Index(0));

        // The completion popup is reset after the deletion.
        let mut app = app_with(":rename foo", CursorPos::Index(1));
        app.command_completion.show_frame = true;
        app.delete_word_after_cursor();
        assert!(!app.command_completion.show_frame());
    }

    #[test]
    fn test_delete_char_after_cursor() {
        // Delete the character under the cursor; the cursor stays in place.
        let mut app = app_with(":rename", CursorPos::Index(1));
        app.delete_char_after_cursor();
        assert_command_line(&app, ":ename", CursorPos::Index(1));

        // Nothing to delete at the end of the command line.
        let mut app = app_with(":rename", CursorPos::End);
        app.delete_char_after_cursor();
        assert_command_line(&app, ":rename", CursorPos::End);

        // A multi-byte character is deleted as a whole.
        let mut app = app_with("文件 x", CursorPos::Index(0));
        app.delete_char_after_cursor();
        assert_command_line(&app, "件 x", CursorPos::Index(0));

        // The completion popup is reset after the deletion.
        let mut app = app_with(":rename", CursorPos::Index(1));
        app.command_completion.show_frame = true;
        app.delete_char_after_cursor();
        assert!(!app.command_completion.show_frame());
    }

    #[test]
    fn test_word_operations_with_multibyte_content() {
        let mut app = app_with("文件 abc", CursorPos::Index(0));

        app.cursor_word_forward();
        assert_command_line(&app, "文件 abc", CursorPos::Index(6));

        app.cursor_word_forward();
        assert_command_line(&app, "文件 abc", CursorPos::End);

        app.cursor_word_backward();
        assert_command_line(&app, "文件 abc", CursorPos::Index(7));

        app.delete_word_before_cursor();
        assert_command_line(&app, "abc", CursorPos::Index(0));
    }

    #[test]
    fn test_word_operations_on_readonly_command_line() {
        let mut app = App {
            selected_block: Block::CommandLine(
                CmdContent::Text(ratatui::text::Text::raw("readonly")),
                CursorPos::None
            ),
            ..Default::default()
        };

        app.cursor_word_forward();
        app.cursor_word_backward();
        app.delete_word_before_cursor();

        assert!(matches!(
            app.selected_block,
            Block::CommandLine(CmdContent::Text(_), CursorPos::None)
        ));
    }

    fn file_saver(name: &str) -> FileSaver {
        let mut file = FileSaver::default();
        file.name = String::from(name);
        file
    }

    #[test]
    fn test_command_line_append_with_multibyte_content() {
        let mut app = app_with("中b", CursorPos::Index(3));

        app.command_line_append('X');
        assert_command_line(&app, "中Xb", CursorPos::Index(4));

        // The cursor moves over the whole inserted character.
        app.command_line_append('文');
        assert_command_line(&app, "中X文b", CursorPos::Index(7));
    }

    #[test]
    fn test_cursor_movement_with_multibyte_content() {
        let mut app = app_with("中文ab", CursorPos::End);

        app.cursor_left(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(7));

        app.cursor_left(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(6));

        app.cursor_left(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(3));

        app.cursor_left(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(0));

        app.cursor_left(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(0));

        app.cursor_right(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(3));

        app.cursor_right(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(6));

        app.cursor_right(false);
        assert_command_line(&app, "中文ab", CursorPos::Index(7));

        app.cursor_right(false);
        assert_command_line(&app, "中文ab", CursorPos::End);

        // The cursor never enters a multi-byte character.
        let mut app = app_with("中文", CursorPos::End);
        app.cursor_left(false);
        assert_command_line(&app, "中文", CursorPos::Index(3));
    }

    #[test]
    fn test_cursor_movement_on_empty_command_line() {
        let mut app = app_with("", CursorPos::End);

        app.cursor_left(false);
        assert_command_line(&app, "", CursorPos::End);

        app.cursor_right(false);
        assert_command_line(&app, "", CursorPos::End);
    }

    #[test]
    fn test_get_content_with_multibyte_content() {
        let app = app_with("中文ab", CursorPos::End);
        let (content, cursor) = get_content(&app.selected_block).unwrap();
        assert_eq!(content, "中文ab");
        assert!(cursor == CursorPos::End);

        let app = app_with("中文ab", CursorPos::Index(3));
        let (content, cursor) = get_content(&app.selected_block).unwrap();
        assert_eq!(content, "中文");
        assert!(cursor == CursorPos::Index(3));

        // An index at the end of the content doesn't panic.
        let app = app_with("中文", CursorPos::Index(6));
        let (content, cursor) = get_content(&app.selected_block).unwrap();
        assert_eq!(content, "中文");
        assert!(cursor == CursorPos::Index(6));
    }

    #[test]
    fn test_completion_position_with_multibyte_content() {
        let mut app = app_with(":create_symlink 中文 a", CursorPos::End);
        app.current_files = vec![file_saver("abc")];

        assert!(completion(&mut app).is_ok());

        // `:create_symlink` and `中文` are 16 and 4 columns wide, so the
        // candidate for `a` starts at column 21 (byte offsets would say 23).
        assert_eq!(app.command_completion.popup_position(), (21, 3));
        assert_command_line(&app, ":create_symlink 中文 abc", CursorPos::End);
    }

    #[test]
    fn test_completion_candidate_width_with_multibyte_content() {
        let mut app = app_with(":create_symlink 中", CursorPos::End);
        app.current_files = vec![file_saver("中文文件名")];

        assert!(matches!(
            update_completion(&mut app, CompletionType::File, "中"),
            Ok(true)
        ));

        // The popup is sized by the display width (five CJK characters).
        assert_eq!(app.command_completion.popup_position().1, 10);
    }
}
