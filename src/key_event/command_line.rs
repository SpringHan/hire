// Operations on command line.

use std::borrow::Cow;

use bitflags::bitflags;
use ratatui::widgets::ListState;

use crate::{app::{App, Block, CursorPos}, error::AppResult, option_get, utils::str_split};

// TODO(remove it): It seems that bitflags is not needed.
bitflags! {
    #[derive(PartialEq, Eq)]
    struct CompletionType: u8 {
        const FILE = 0b0000_0001;
        const SHELL = 0b0000_0010;
        const BUILTIN = 0b0000_0100;
    }
}

#[derive(Default)]
pub struct AppCompletion<'a> {
    selected_item: ListState,
    candidates: Vec<Cow<'a, str>>,
}

pub fn completion(app: &mut App) -> AppResult<()> {
    let content_info = get_content(&app.selected_block);
    if content_info.is_none() {
        return Ok(())
    }

    let (content, _) = content_info.unwrap();
    let command_slice: Vec<&str> = content.split(" ").collect();

    let updated = match command_slice.len() {
        1 => {
            if !command_slice[0].starts_with(":!") &&
                command_slice[0].starts_with(":")
            {
                update_completion(
                    app,
                    CompletionType::BUILTIN,
                    &command_slice[0][1..]
                )
            } else {
                false
            }
        },

        2 => update_completion(
            app,
            if command_slice[0].starts_with(":!") {
                CompletionType::SHELL | CompletionType::FILE
            } else {
                CompletionType::FILE
            },
            command_slice[1]
        ),

        _ => update_completion(
            app,
            CompletionType::FILE,
            command_slice.last().unwrap()
        )
    };

    if updated {
        update_cmdline(
            &mut app.selected_block,
            &mut app.command_completion,
        )?;
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
        completion.candidates.clear();
        completion.selected_item.select(None);
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
        *command_slice.last_mut().unwrap() = new_value.as_ref().into();
    }

    let updated_content = command_slice.join(" ");
    if cursor != CursorPos::End {
        cursor = CursorPos::Index(updated_content.len());
    }

    app_block.set_command_line(updated_content, cursor);

    Ok(())
}

/// Return true if the completion candidates is updated.
fn update_completion(
    app: &mut App,
    _type: CompletionType,
    current: &str
) -> bool
{
    let mut candidates: Vec<Cow<str>> = Vec::new();

    if _type.contains(CompletionType::FILE) {
        let files_iter = if app.root() {
            app.parent_files.iter()
        } else {
            app.current_files.iter()
        };
        
        for file in files_iter {
            if file.name.starts_with(current) {
                candidates.push(Cow::Owned(file.name.to_owned()));
            }
        }
    }

    // NOTE: I don't think the program need a shell completion.
    // if _type.contains(CompletionType::SHELL) {
    //     let bin = read_dir("/usr/bin")?;

    //     for file in bin {
    //     }
    // }

    if _type == CompletionType::BUILTIN {
        let commands = ["rename", "create_file", "create_dir", "create_symlink"];

        for cmd in commands.into_iter() {
            if cmd.starts_with(current) {
                candidates.push(Cow::Borrowed(cmd));
            }
        }
    }

    if candidates.is_empty() {
        return false
    }

    candidates.sort_by(|a, b| a.len().cmp(&b.len()));

    let completion = &mut app.command_completion;
    completion.candidates = candidates;
    completion.selected_item.select(Some(0));

    true
}

fn get_content(app_block: &Block) -> Option<(String, CursorPos)> {
    let (_content, cursor) = if let Block::CommandLine(
        ref cont,
        pos
    ) = *app_block {
        (cont.to_owned(), pos)
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
