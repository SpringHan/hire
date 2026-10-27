// Utils for crate.

mod types;

use std::{borrow::Cow, fs::File, io::Read, sync::atomic::{AtomicU16, Ordering}};

use anyhow::bail;
use clap::Parser;
use ratatui::text::{Span, Text};
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

    /// Internal: restore the tabs handed over by a sudo session.
    #[arg(long, hide = true, default_value_t = String::from("NULL"))]
    pub sudo_session: String,

    /// Internal: the isolated config directory used by a sudo session.
    #[arg(long, hide = true, default_value_t = String::from("NULL"))]
    pub config_dir: String,
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
///
/// The character right before the cursor decides how much is deleted: a word
/// separator is deleted alone, otherwise the whole word in front of the cursor
/// is removed. The word separators are the ASCII punctuation characters, which
/// is the rule this function has always used (see [`is_delete_word_separator`]);
/// non-ASCII characters are word characters and are never split.
pub fn delete_word(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    // Find the start of the edited slice: either the separator right before the
    // cursor, or the beginning of the word in front of it.
    let mut start = pos;
    for (idx, _char) in _str[..pos].char_indices().rev() {
        if is_delete_word_separator(_char) {
            if start == pos {
                start = idx;
            }

            break
        }

        start = idx;
    }

    if start == pos {
        return
    }

    let remain = _str[pos..].to_owned();
    _str.truncate(start);
    _str.push_str(&remain);
    *cursor = cursor_from_insertion_point(_str, start);
}

/// Check whether the character separates words for [`delete_word`].
///
/// The separator set is the ASCII punctuation range, which is the rule
/// [`delete_word`] has always used. It's kept separate from [`is_word_char`],
/// which drives the word-wise cursor movement and word deletions.
fn is_delete_word_separator(_char: char) -> bool {
    matches!(
        _char,
        '\x20'..='\x2f' | '\x3a'..='\x40' | '\x5b'..='\x60' | '\x7b'..='\x7e'
    )
}

/// Check whether the character is a part of a word when the cursor moves or
/// deletes word by word.
///
/// Whitespace and punctuation like `:` or `.` separate words, so a file name
/// such as `foo_bar.txt` is made of the words `foo_bar` and `txt`. Non-ASCII
/// letters (e.g. CJK characters) are word characters.
pub fn is_word_char(_char: char) -> bool {
    _char.is_alphanumeric() || _char == '_'
}

/// Convert a cursor position into the insertion point of a text.
///
/// The returned value is the byte index at which new characters are inserted,
/// which may equal the length of `content` when the cursor is at its end.
/// `None` is returned when there is no cursor or the cursor points inside a
/// multi-byte character.
pub fn cursor_insertion_point(content: &str, cursor: CursorPos) -> Option<usize> {
    let pos = match cursor {
        CursorPos::Index(idx) => idx,
        CursorPos::End => content.len(),
        CursorPos::None => return None,
    };

    if pos <= content.len() && content.is_char_boundary(pos) {
        Some(pos)
    } else {
        None
    }
}

/// Return the byte index of the nearest character boundary before `pos`.
///
/// `None` is returned when `pos` is 0 or doesn't lie on a character boundary.
pub fn prev_char_boundary(content: &str, pos: usize) -> Option<usize> {
    if pos == 0 || pos > content.len() || !content.is_char_boundary(pos) {
        return None
    }

    content[..pos].char_indices().next_back().map(|(idx, _)| idx)
}

/// Return the byte index of the nearest character boundary after `pos`.
///
/// The result may be the length of `content`, which means the cursor reaches
/// the end of it. `None` is returned when `pos` is at the end or doesn't lie on
/// a character boundary.
pub fn next_char_boundary(content: &str, pos: usize) -> Option<usize> {
    if pos >= content.len() || !content.is_char_boundary(pos) {
        return None
    }

    let _char = content[pos..].chars().next()?;
    Some(pos + _char.len_utf8())
}

/// Build the cursor state for an insertion point inside `content`.
///
/// `CursorPos::End` is used when the insertion point is placed after the last
/// character, which keeps the cursor valid while characters are appended.
pub fn cursor_from_insertion_point(content: &str, pos: usize) -> CursorPos {
    if pos >= content.len() {
        CursorPos::End
    } else {
        CursorPos::Index(pos)
    }
}

/// Return the byte index right after the last character of the nearest word at
/// or after `start`, and the length of `content` when there is no word left.
pub fn next_word_end(content: &str, start: usize) -> usize {
    let mut end = start;
    let mut in_word = false;

    for (idx, _char) in content[start..].char_indices() {
        if !is_word_char(_char) {
            if in_word {
                break
            }

            continue
        }

        in_word = true;
        end = start + idx + _char.len_utf8();
    }

    if in_word {
        end
    } else {
        content.len()
    }
}

/// Return the byte index of the first character of the nearest word before
/// `start`, and 0 when there is none.
pub fn prev_word_start(content: &str, start: usize) -> usize {
    let mut word_start = start;
    let mut in_word = false;

    for (idx, _char) in content[..start].char_indices().rev() {
        if !is_word_char(_char) {
            // The separators right before the cursor are skipped first, and the
            // first separator after the word marks the word boundary.
            if in_word {
                break
            }

            continue
        }

        in_word = true;
        word_start = idx;
    }

    // There is no word but only separators before the cursor: the beginning of
    // the content is the nearest boundary.
    if in_word {
        word_start
    } else {
        0
    }
}

/// Move the cursor to the end of the nearest word at or after the cursor.
///
/// The word separators are skipped first, so the cursor lands right after the
/// last character of the next word, and at the end of the content when there is
/// no word left.
pub fn cursor_next_word_end(content: &str, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(content, *cursor) else {
        return
    };

    *cursor = cursor_from_insertion_point(content, next_word_end(content, pos));
}

/// Move the cursor to the start of the nearest word before the cursor.
///
/// The word separators right before the cursor are skipped first, so the cursor
/// lands on the first character of the previous word, and at the beginning of
/// the content when there is no word left.
pub fn cursor_prev_word_start(content: &str, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(content, *cursor) else {
        return
    };

    *cursor = cursor_from_insertion_point(content, prev_word_start(content, pos));
}

/// Delete the whole word before the cursor and update content & cursor
/// position.
///
/// The deleted range starts at the word start found by [`prev_word_start`], so
/// the separators between the word and the cursor are removed as well:
/// `:rename foo.txt` becomes `:rename foo.` when the cursor is placed after
/// `txt`.
///
/// Unlike [`delete_word`], the word boundaries are the same as the word-wise
/// cursor movement, and the content is never cut inside a multi-byte character.
pub fn delete_word_before_cursor(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    let start = prev_word_start(_str, pos);
    if start == pos {
        return
    }

    _str.replace_range(start..pos, "");
    *cursor = cursor_from_insertion_point(_str, start);
}

/// Delete the whole word after the cursor and update content & cursor position.
///
/// The deleted range ends at the word end found by [`next_word_end`], so the
/// separators between the cursor and the word are removed as well:
/// `:rename foo.txt` becomes `:rename .txt` when the cursor is placed before
/// `foo`.
///
/// The word boundaries are the same as the word-wise cursor movement, and the
/// content is never cut inside a multi-byte character.
pub fn delete_word_after_cursor(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    let end = next_word_end(_str, pos);
    if end == pos {
        return
    }

    _str.replace_range(pos..end, "");
    *cursor = cursor_from_insertion_point(_str, pos);
}

/// Delete the character after the cursor and update content & cursor position.
///
/// The cursor stays at the same insertion point, so repeating the operation
/// deletes the following characters one by one. A multi-byte character is
/// removed as a whole, and nothing happens when the cursor is at the end of the
/// content.
pub fn delete_char_after_cursor(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    let Some(_char) = _str[pos..].chars().next() else {
        return
    };

    _str.replace_range(pos..pos + _char.len_utf8(), "");
    *cursor = cursor_from_insertion_point(_str, pos);
}

/// Return the width of `text` when it's rendered in the terminal.
///
/// Wide characters (e.g. CJK ones) take two columns and combining marks take
/// none, so neither the byte length nor the character count can be used for
/// layout. Only this width can be used to position the completion popup and the
/// command line cursor.
pub fn display_width(text: &str) -> usize {
    Span::raw(text).width()
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_word_char() {
        assert!(is_word_char('a'));
        assert!(is_word_char('0'));
        assert!(is_word_char('_'));
        assert!(is_word_char('文'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('/'));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('-'));
    }

    #[test]
    fn test_next_word_end() {
        let content = "hello world";

        // From the beginning and from inside a word, the end of that word.
        assert_eq!(next_word_end(content, 0), 5);
        assert_eq!(next_word_end(content, 2), 5);

        // From a separator, the end of the next word.
        assert_eq!(next_word_end(content, 5), 11);
        assert_eq!(next_word_end(content, 6), 11);
        assert_eq!(next_word_end(content, 7), 11);

        // There is no word left.
        assert_eq!(next_word_end(content, 11), 11);
        assert_eq!(next_word_end("hello  ", 5), 7);
    }

    #[test]
    fn test_prev_word_start() {
        let content = "hello world";

        // From the end and from inside a word, the start of that word.
        assert_eq!(prev_word_start(content, 11), 6);
        assert_eq!(prev_word_start(content, 8), 6);

        // From a word start, the start of the previous word.
        assert_eq!(prev_word_start(content, 6), 0);
        assert_eq!(prev_word_start(content, 5), 0);

        // There is no word before the cursor.
        assert_eq!(prev_word_start(content, 0), 0);
        assert_eq!(prev_word_start("  hello", 2), 0);
    }

    #[test]
    fn test_cursor_insertion_point() {
        assert!(cursor_insertion_point("abc", CursorPos::End) == Some(3));
        assert!(cursor_insertion_point("abc", CursorPos::Index(1)) == Some(1));
        assert!(cursor_insertion_point("abc", CursorPos::None).is_none());

        // The cursor cannot point outside the content or inside a character.
        assert!(cursor_insertion_point("abc", CursorPos::Index(4)).is_none());
        assert!(cursor_insertion_point("文件", CursorPos::Index(1)).is_none());
    }

    #[test]
    fn test_cursor_from_insertion_point() {
        assert!(cursor_from_insertion_point("abc", 0) == CursorPos::Index(0));
        assert!(cursor_from_insertion_point("abc", 2) == CursorPos::Index(2));
        assert!(cursor_from_insertion_point("abc", 3) == CursorPos::End);
        assert!(cursor_from_insertion_point("", 0) == CursorPos::End);
    }

    #[test]
    fn test_cursor_word_movement() {
        let content = "hello world";
        let mut cursor = CursorPos::Index(0);

        cursor_next_word_end(content, &mut cursor);
        assert!(cursor == CursorPos::Index(5));

        cursor_next_word_end(content, &mut cursor);
        assert!(cursor == CursorPos::End);

        cursor_prev_word_start(content, &mut cursor);
        assert!(cursor == CursorPos::Index(6));

        cursor_prev_word_start(content, &mut cursor);
        assert!(cursor == CursorPos::Index(0));

        // Nothing happens when there is no cursor.
        let mut cursor = CursorPos::None;
        cursor_next_word_end(content, &mut cursor);
        cursor_prev_word_start(content, &mut cursor);
        assert!(cursor == CursorPos::None);
    }

    #[test]
    fn test_delete_word_before_cursor() {
        // Delete the word in the middle of the content.
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(8);

        delete_word_before_cursor(&mut content, &mut cursor);
        assert_eq!(content, "hello rld");
        assert!(cursor == CursorPos::Index(6));

        // Multi-byte characters are never cut.
        let mut content = String::from("文件 abc");
        let mut cursor = CursorPos::End;

        delete_word_before_cursor(&mut content, &mut cursor);
        assert_eq!(content, "文件 ");
        assert!(cursor == CursorPos::End);

        // Nothing to delete at the beginning of the content.
        let mut content = String::from("abc");
        let mut cursor = CursorPos::Index(0);

        delete_word_before_cursor(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::Index(0));
    }

    #[test]
    fn test_delete_char_after_cursor() {
        // Delete the character under the cursor, the cursor stays in place.
        let mut content = String::from("abc");
        let mut cursor = CursorPos::Index(1);

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "ac");
        assert!(cursor == CursorPos::Index(1));

        // Repeated deletions remove the following characters one by one.
        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "a");
        assert!(cursor == CursorPos::End);

        // Nothing to delete at the end of the content.
        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "a");
        assert!(cursor == CursorPos::End);

        // A multi-byte character is removed as a whole.
        let mut content = String::from("文件abc");
        let mut cursor = CursorPos::Index(0);

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "件abc");
        assert!(cursor == CursorPos::Index(0));

        // Nothing happens without a cursor.
        let mut content = String::from("abc");
        let mut cursor = CursorPos::None;

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::None);
    }

    #[test]
    fn test_delete_word_after_cursor() {
        // From inside a word, the rest of that word is deleted.
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(2);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "he world");
        assert!(cursor == CursorPos::Index(2));

        // From a separator, the separator and the next word are deleted.
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(5);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "hello");
        assert!(cursor == CursorPos::End);

        // Nothing to delete at the end of the content.
        let mut content = String::from("hello");
        let mut cursor = CursorPos::End;

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "hello");
        assert!(cursor == CursorPos::End);

        // Multi-byte characters are never cut.
        let mut content = String::from("文件 abc");
        let mut cursor = CursorPos::Index(0);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, " abc");
        assert!(cursor == CursorPos::Index(0));

        // Nothing happens without a cursor.
        let mut content = String::from("abc");
        let mut cursor = CursorPos::None;

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::None);
    }

    #[test]
    fn test_prev_char_boundary() {
        assert_eq!(prev_char_boundary("abc", 3), Some(2));
        assert_eq!(prev_char_boundary("abc", 1), Some(0));
        assert_eq!(prev_char_boundary("abc", 0), None);
        assert_eq!(prev_char_boundary("", 0), None);

        // A CJK character takes three bytes.
        assert_eq!(prev_char_boundary("中文", 6), Some(3));
        assert_eq!(prev_char_boundary("中文", 3), Some(0));

        // The position has to be a character boundary of the content.
        assert_eq!(prev_char_boundary("中", 1), None);
        assert_eq!(prev_char_boundary("abc", 4), None);
    }

    #[test]
    fn test_next_char_boundary() {
        assert_eq!(next_char_boundary("abc", 0), Some(1));
        assert_eq!(next_char_boundary("abc", 2), Some(3));
        assert_eq!(next_char_boundary("abc", 3), None);
        assert_eq!(next_char_boundary("", 0), None);

        // A CJK character takes three bytes.
        assert_eq!(next_char_boundary("中文", 0), Some(3));
        assert_eq!(next_char_boundary("中文", 3), Some(6));

        assert_eq!(next_char_boundary("中", 1), None);
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("中文"), 4);
        assert_eq!(display_width("a中b"), 4);
    }

    #[test]
    fn test_delete_word() {
        // The ASCII behaviour is preserved.
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(8);
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "hello rld");
        assert!(cursor == CursorPos::Index(6));

        let mut content = String::from("abc");
        let mut cursor = CursorPos::End;
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "");
        assert!(cursor == CursorPos::End);

        // Only the separator right before the cursor is deleted.
        let mut content = String::from("abc..");
        let mut cursor = CursorPos::End;
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "abc.");
        assert!(cursor == CursorPos::End);

        // The word separators in front of the deleted word are kept.
        let mut content = String::from("abc.def");
        let mut cursor = CursorPos::Index(7);
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "abc.");
        assert!(cursor == CursorPos::End);

        // Multi-byte characters are never split.
        let mut content = String::from("中文");
        let mut cursor = CursorPos::End;
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "");
        assert!(cursor == CursorPos::End);

        let mut content = String::from("中文.txt");
        let mut cursor = CursorPos::Index(6);
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, ".txt");
        assert!(cursor == CursorPos::Index(0));

        let mut content = String::from("中文 x");
        let mut cursor = CursorPos::Index(6);
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, " x");
        assert!(cursor == CursorPos::Index(0));
    }
}
