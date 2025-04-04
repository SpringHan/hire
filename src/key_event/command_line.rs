// Operations on command line.

use std::{borrow::Cow, ops::{AddAssign, SubAssign}};

use ratatui::widgets::ListState;

use crate::{
    app::{App, Block, CursorPos},
    error::AppResult,
    utils::str_split,
    option_get,
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
