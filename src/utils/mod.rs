// Utils for crate.

mod types;

use std::{borrow::Cow, fs::File, io::Read, sync::atomic::{AtomicU16, Ordering}};

use anyhow::bail;
use clap::Parser;
use ratatui::text::Text;
use ansi_to_tui::IntoText;
use lazy_static::lazy_static;

pub use types::*;

lazy_static! {
    /// The height of file list & content preview windows.
    pub static ref WINDOW_HEIGHT: AtomicU16 = AtomicU16::new(0);
}

#[derive(Parser)]
pub struct Args {
    /// Directly spawn a shell in the working directory
    #[arg(
        short,
        long,
        default_value_t = false,
        conflicts_with = "start_from"
    )]
    pub working_directory: bool,

    #[arg(
        short,
        long,
        default_value_t = String::from("NULL"),
        conflicts_with = "working_directory"
    )]
    pub start_from: String,

    /// The target file for output, require a absolute path. The default: /tmp/hire.tmp
    #[arg(short, long, default_value_t = String::from("NULL"))]
    pub output_file: String,

    /// Kill the hire process after an output operation.
    #[arg(short, long, default_value_t = false)]
    pub quit_after_output: bool,
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right
}

impl Direction {
    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        Ok(
            match value {
                "up" => Self::Up,
                "down" => Self::Down,
                "left" => Self::Left,
                "right" => Self::Right,
                _ => bail!("Unknow keyword to parse into Direction!")
            }
        )
    }
}

/// Split String into Vec<String>
pub fn str_split(_string: String) -> Vec<String> {
    let mut str_vec: Vec<String> = Vec::new();

    for _str in _string.split(" ") {
        str_vec.push(_str.to_owned());
    }

    str_vec
}

/// Read limited lines from file, and pass content as `Text` structure to `text_ref`.
/// In the meanwhile, the newline character of Windows will be removed
/// and the '\t' will be replaced with 4 spaces.
pub fn read_to_text(
    text_ref: &mut Text,
    file: &File,
) -> anyhow::Result<()>
{
    let line_nr = get_window_height();

    let mut idx = 1;
    let mut bytes: Vec<u8> = Vec::new();
    for _b in file.bytes() {
        let byte = _b?;
        if byte != 13 {
            // To limit content read from file
            if byte == 10 {
                if idx == line_nr {
                    break;
                }

                idx += 1;
            }

            bytes.push(byte);
        }
    }

    let _string = String::from_utf8(bytes)?;
    let mut text = _string.into_text()?;

    for line in text.iter_mut() {
        for span in line.iter_mut() {
            span.content = Cow::Owned(span.content.replace("\t", "    "));
        }
    }

    *text_ref = text;

    Ok(())
}

/// Delete word and update content & cursor position.
pub fn delete_word(_str: &mut String, cursor: &mut CursorPos) {
    if _str.len() == 0 {
        return ()
    }

    let mut end_idx: usize;
    let mut deleting_char = false;
    let mut remain_str = String::new();

    if let CursorPos::Index(idx) = cursor {
        if *idx == 0 {
            return ()
        }
        end_idx = *idx - 1;
        remain_str = _str[*idx..].to_owned();
    } else {
        end_idx = _str.len() - 1;
    }

    // Get the end position of editing slice after deleting
    for byte in _str
        .bytes()
        .rev()
        .skip(_str.len() - end_idx - 1)
    {

        if (byte >= 32 && byte <= 47) ||
            (byte >= 58 && byte <= 64) ||
            (byte >= 91 && byte <= 96) ||
            (byte >= 123 && byte <= 126)
        {
            if !deleting_char {
                end_idx = end_idx.saturating_sub(1);
            }

            break;
        }

        end_idx = end_idx.saturating_sub(1);
        if !deleting_char {
            deleting_char = true;
        }
    }

    // Update content & cursor position
    if end_idx == 0 {
        _str.clear();
    } else {
        *_str = _str[0..=end_idx].to_owned();
    }
    if !remain_str.is_empty() {
        _str.push_str(&remain_str);
    }

    if let CursorPos::Index(idx) = cursor {
        *idx = if end_idx == 0 { 0 } else { end_idx + 1 };
    }
}

pub fn update_window_height(height: u16) {
    let current = WINDOW_HEIGHT.load(Ordering::Acquire);
    if current != height {
        WINDOW_HEIGHT.store(height, Ordering::Release);
    }
}

pub fn get_window_height() -> u16 {
    WINDOW_HEIGHT.load(Ordering::Acquire)
}
