// Operations on command line.

use std::{borrow::Cow, ops::{AddAssign, SubAssign}};

use ratatui::widgets::ListState;

use crate::{
    utils::{Block, CmdContent, CursorPos},
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

                position += command_slice[i].len() + 1;
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

    let (content, mut cursor) = content_info.unwrap();
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
    if cursor != CursorPos::End {
        cursor = CursorPos::Index(updated_content.len());
    }

    app_block.set_command_line(updated_content, cursor);

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

    // Get content slice
    let idx = match cursor {
        CursorPos::Index(idx) => idx,
        CursorPos::End => _content.len() - 1,
        CursorPos::None => return None,
    };
    let content = &_content[..=idx];

    Some((content.to_owned(), cursor))
}

impl<'a> App<'a> {
    pub fn cursor_left(&mut self, edge: bool) {
        if let Block::CommandLine(
            ref command,
            ref mut cursor
        ) = self.selected_block
        {
            if edge {
                *cursor = CursorPos::Index(0);
                if self.command_completion.show_frame {
                    self.command_completion.reset();
                }

                return ()
            }

            match cursor {
                CursorPos::Index(idx) => {
                    if *idx == 0 {
                        return ()
                    }
                    idx.sub_assign(1);
                },
                CursorPos::End => {
                    *cursor = CursorPos::Index(
                        command.get().len() - 1
                    );
                },
                _ => ()
            }

            if self.command_completion.show_frame {
                self.command_completion.reset();
            }
        }
    }

    pub fn cursor_right(&mut self, edge: bool) {
        if let Block::CommandLine(
            ref command,
            ref mut cursor
        ) = self.selected_block
        {
            if edge {
                *cursor = CursorPos::End;
                if self.command_completion.show_frame {
                    self.command_completion.reset();
                }

                return ()
            }

            if let CursorPos::Index(idx) = cursor {
                if *idx == command.get().len() - 1 {
                    *cursor = CursorPos::End;
                } else {
                    idx.add_assign(1);
                }

                if self.command_completion.show_frame {
                    self.command_completion.reset();
                }
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
        match cursor {
            CursorPos::Index(idx) => {
                if *idx > content.len() {
                    return false
                }

                content.insert_str(*idx, text);

                // When the cursor reaches the end of the content, use `End`
                // instead of an index which is out of the content.
                let new_idx = *idx + text.len();
                *cursor = if new_idx == content.len() {
                    CursorPos::End
                } else {
                    CursorPos::Index(new_idx)
                };
            },
            CursorPos::End => content.push_str(text),
            CursorPos::None => return false,
        }

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
                if max_width < file.name.len() {
                    max_width = file.name.len();
                }
                candidates.push(Cow::Owned(file.name.to_owned()));
            }
        }
    } else {
        let commands = ["rename", "create_file", "create_dir", "create_symlink"];

        for cmd in commands.into_iter() {
            if cmd.starts_with(current) {
                if max_width < cmd.len() {
                    max_width = cmd.len();
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

    fn command_block(content: &str, cursor: CursorPos) -> Block {
        Block::CommandLine(
            CmdContent::String(content.to_owned()),
            cursor
        )
    }

    fn command_content(app_block: &Block) -> &str {
        if let Block::CommandLine(CmdContent::String(ref content), _) = *app_block {
            content
        } else {
            panic!("The block is not an editable command line!")
        }
    }

    fn command_cursor(app_block: &Block) -> CursorPos {
        if let Block::CommandLine(_, cursor) = *app_block {
            cursor
        } else {
            panic!("The block is not a command line!")
        }
    }

    #[allow(clippy::field_reassign_with_default)]
    fn app_with_files(selected: usize, files: &[&str]) -> App<'static> {
        let mut app = App::default();
        app.current_files = files
            .iter()
            .map(|name| {
                let mut file = FileSaver::default();
                file.name = String::from(*name);
                file
            })
            .collect();
        app.selected_item.current_select(Some(selected));
        app
    }

    #[test]
    fn test_command_line_after_space() {
        // The cursor is at the end and right after a space.
        assert!(command_line_after_space(
            &command_block(":create_file ", CursorPos::End)
        ));

        // The cursor is on the character right after a space.
        assert!(command_line_after_space(
            &command_block(":create_file test", CursorPos::Index(13))
        ));

        // The cursor is on a character which is not right after a space.
        assert!(!command_line_after_space(
            &command_block(":create_file test", CursorPos::Index(14))
        ));

        // The cursor is at the beginning of the command line.
        assert!(!command_line_after_space(
            &command_block(":create_file ", CursorPos::Index(0))
        ));

        // The cursor is at the end without a space before it.
        assert!(!command_line_after_space(
            &command_block(":create_file", CursorPos::End)
        ));

        // The command line is not editable.
        assert!(!command_line_after_space(
            &Block::CommandLine(
                CmdContent::String(String::from(":create_file ")),
                CursorPos::None
            )
        ));

        // The block is not a command line.
        assert!(!command_line_after_space(&Block::Browser(false)));
    }

    #[test]
    fn test_insert_before_cursor() {
        // Insert before the character under the cursor.
        let mut block = command_block(
            ":create_file test",
            CursorPos::Index(13)
        );
        assert!(insert_before_cursor(&mut block, "name"));
        assert_eq!(command_content(&block), ":create_file nametest");
        assert!(matches!(
            command_cursor(&block),
            CursorPos::Index(17)
        ));

        // Insert at the end of the command line.
        let mut block = command_block(":create_file ", CursorPos::End);
        assert!(insert_before_cursor(&mut block, "test"));
        assert_eq!(command_content(&block), ":create_file test");
        assert!(matches!(command_cursor(&block), CursorPos::End));

        // The cursor is set to `End` when the inserted text reaches the end
        // of the command line, even if the cursor was an index.
        let mut block = command_block(":create_file ", CursorPos::Index(13));
        assert!(insert_before_cursor(&mut block, "test"));
        assert_eq!(command_content(&block), ":create_file test");
        assert!(matches!(command_cursor(&block), CursorPos::End));

        // Refuse to insert into a non-editable command line.
        let mut block = Block::CommandLine(
            CmdContent::String(String::from(":create_file ")),
            CursorPos::None
        );
        assert!(!insert_before_cursor(&mut block, "test"));
        assert_eq!(command_content(&block), ":create_file ");

        // Refuse to insert into another kind of block.
        assert!(!insert_before_cursor(&mut Block::Browser(false), "test"));
    }

    #[test]
    fn test_complete_selected_item() {
        // The name of the selected item is inserted when the cursor is right
        // after a space.
        let mut app = app_with_files(1, &["aaa.txt", "target.txt"]);
        app.selected_block = command_block(":!cat ", CursorPos::End);

        completion(&mut app).unwrap();
        assert_eq!(
            command_content(&app.selected_block),
            ":!cat target.txt"
        );
        assert!(matches!(
            command_cursor(&app.selected_block),
            CursorPos::End
        ));
        assert!(app.command_completion.candidates.is_empty());

        // The name is inserted before the cursor when the cursor is in the
        // middle of the command line.
        app.selected_block = command_block(":!cat  --help", CursorPos::Index(6));
        completion(&mut app).unwrap();
        assert_eq!(
            command_content(&app.selected_block),
            ":!cat target.txt --help"
        );
        assert!(matches!(
            command_cursor(&app.selected_block),
            CursorPos::Index(16)
        ));
    }

    #[test]
    fn test_original_completion_kept() {
        // The builtin completion still works when there's no space before the
        // cursor.
        let mut app = app_with_files(1, &["aaa.txt", "target.txt"]);
        app.selected_block = command_block(":ren", CursorPos::End);

        completion(&mut app).unwrap();
        assert_eq!(command_content(&app.selected_block), ":rename");
        assert_eq!(app.command_completion.candidates.len(), 1);

        // The file completion still works for a partial word.
        let mut app = app_with_files(1, &["aaa.txt", "target.txt"]);
        app.selected_block = command_block(":!cat ta", CursorPos::End);

        completion(&mut app).unwrap();
        assert_eq!(
            command_content(&app.selected_block),
            ":!cat target.txt"
        );
        assert_eq!(app.command_completion.candidates.len(), 1);
    }
}
