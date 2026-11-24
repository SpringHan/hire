//! Shared helpers: `Args`, cursor/word editing, file probing, and previews.

mod preview;
mod syntax;
mod types;

use std::{
    borrow::Cow,
    fs::File,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU16, Ordering}
};

use anyhow::bail;
use clap::Parser;
use ratatui::text::{Span, Text};
use ansi_to_tui::IntoText;
use lazy_static::lazy_static;

pub use types::*;
pub use syntax::*;
pub use preview::*;

lazy_static! {
    /// The height of the file list and content preview panes.
    ///
    /// The pane size is only known while drawing, so it is stored here for the
    /// key handlers and list rendering which read it later.
    pub static ref WINDOW_HEIGHT: AtomicU16 = AtomicU16::new(0);

    /// The width of the content preview pane, which bounds its horizontal
    /// scrolling.
    pub static ref WINDOW_WIDTH: AtomicU16 = AtomicU16::new(0);
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

    /// Start the file browser in this directory
    #[arg(
        short,
        long,
        default_value_t = String::from("NULL"),
        conflicts_with = "working_directory"
    )]
    pub start_from: String,

    /// Directly spawn a shell in the directory hire starts in.
    ///
    /// Together with `--start-from <path>` the shell runs in `<path>`.
    /// `--working-directory` already opens a shell, in the stored working
    /// directory instead, so it cannot be combined with this flag.
    #[arg(
        short = 'S',
        long,
        default_value_t = false,
        conflicts_with = "working_directory"
    )]
    pub shell: bool,

    /// The target file for output, require a absolute path. The default: /tmp/hire.tmp
    #[arg(short, long, default_value_t = String::from("NULL"))]
    pub output_file: String,

    /// Kill the hire process after an output operation.
    #[arg(short, long, default_value_t = false)]
    pub quit_after_output: bool,

    /// Print the version of hire and exit
    #[arg(short, long, default_value_t = false)]
    pub version: bool,

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

pub fn str_split(_string: String) -> Vec<String> {
    let mut str_vec: Vec<String> = Vec::new();

    for _str in _string.split(" ") {
        str_vec.push(_str.to_owned());
    }

    str_vec
}

/// Turn `content` into `Text` with its ANSI colours, expanding each tab to four
/// spaces (a tab is a control character inside a `Span`).
pub fn plain_text(content: &str) -> anyhow::Result<Text<'static>> {
    let mut text = content.into_text()?;

    for line in text.iter_mut() {
        for span in line.iter_mut() {
            span.content = Cow::Owned(span.content.replace("\t", "    "));
        }
    }

    Ok(text)
}

/// Whether `path` is shown as text by the preview.
///
/// Only the probe prefix is read, and the same test picks the program the file
/// opens with: an editor for text, the desktop handler otherwise.
pub fn is_text_file(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false
    };

    read_probe(&file).is_ok()
}

/// The MIME type `xdg-mime` reports for `path`, e.g. `application/pdf`.
///
/// `None` when the query fails or answers nothing.
pub fn file_mime(path: &Path) -> Option<String> {
    let output = Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None
    }

    let mime = String::from_utf8(output.stdout).ok()?;
    let mime = mime.trim();

    if mime.is_empty() {
        None
    } else {
        Some(mime.to_owned())
    }
}

/// What the preview shows for a file which is not text: its MIME type, or the
/// old label when `xdg-mime` cannot answer.
pub fn file_type_label(path: &Path) -> String {
    file_mime(path).unwrap_or_else(|| String::from("Non Text File"))
}

/// The indices of the names which match `query`.
///
/// `exactly` picks the first name equal to the query (the jump-to-file search),
/// otherwise every case-insensitive substring match. The file list and the
/// preview lines share the rule, hence strings instead of file types.
pub fn search_indexes<'a, I>(query: &str, names: I, exactly: bool) -> Vec<usize>
where I: IntoIterator<Item = &'a str>
{
    let query = if !exactly {
        query.to_lowercase()
    } else {
        query.to_owned()
    };

    let mut indexes: Vec<usize> = Vec::new();

    for (idx, name) in names.into_iter().enumerate() {
        if exactly {
            if name == query.as_str() {
                indexes.push(idx);
                break
            }

            continue
        }

        if name.to_lowercase().contains(&query) {
            indexes.push(idx);
        }
    }

    indexes
}

/// The byte index at which the extension of `name` starts, or `None`.
///
/// The extension starts at the last `.`, except when it is the leading one:
/// `".bashrc"` is a hidden file, not an extension. The index is a byte offset,
/// so it is always on a character boundary.
pub fn extension_start(name: &str) -> Option<usize> {
    match name.rfind('.') {
        Some(idx) if idx > 0 => Some(idx),
        _ => None,
    }
}

/// Delete the word before the cursor and update the content and the cursor.
///
/// A word separator right before the cursor is deleted alone, otherwise the
/// whole word is removed. The separator set is the ASCII punctuation range,
/// which is the rule this function has always used (see
/// [`is_delete_word_separator`]).
pub fn delete_word(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    // Find the start of the edited slice.
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

/// Whether the character separates words for [`delete_word`].
///
/// The ASCII punctuation range is the rule [`delete_word`] has always used; it
/// is deliberately not [`is_word_char`], which drives word-wise movement.
fn is_delete_word_separator(_char: char) -> bool {
    matches!(
        _char,
        '\x20'..='\x2f' | '\x3a'..='\x40' | '\x5b'..='\x60' | '\x7b'..='\x7e'
    )
}

/// Whether the character is part of a word for word-wise movement and deletion.
///
/// Whitespace and punctuation such as `:`, `.` or `_` separate words, so
/// `foo_bar.txt` is three words; non-ASCII letters (CJK included) are word
/// characters.
pub fn is_word_char(_char: char) -> bool {
    _char.is_alphanumeric()
}

/// The byte index at which the cursor would insert, or `None`.
///
/// The index may equal the length of `content` (the cursor is at its end), but
/// never points inside a multi-byte character.
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

/// The byte index of the character boundary before `pos`, or `None` when `pos`
/// is 0 or not a boundary.
pub fn prev_char_boundary(content: &str, pos: usize) -> Option<usize> {
    if pos == 0 || pos > content.len() || !content.is_char_boundary(pos) {
        return None
    }

    content[..pos].char_indices().next_back().map(|(idx, _)| idx)
}

/// The byte index of the character boundary after `pos`, or `None` when `pos`
/// is at the end or not a boundary.
pub fn next_char_boundary(content: &str, pos: usize) -> Option<usize> {
    if pos >= content.len() || !content.is_char_boundary(pos) {
        return None
    }

    let _char = content[pos..].chars().next()?;
    Some(pos + _char.len_utf8())
}

/// Turn an insertion point inside `content` into a cursor position.
///
/// A point after the last character becomes `End`, which stays valid while
/// characters are appended.
pub fn cursor_from_insertion_point(content: &str, pos: usize) -> CursorPos {
    if pos >= content.len() {
        CursorPos::End
    } else {
        CursorPos::Index(pos)
    }
}

/// The byte index just after the word at or after `start`, or the length of
/// `content` when no word is left.
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

/// The byte index of the first character of the word before `start`, or 0 when
/// there is none.
pub fn prev_word_start(content: &str, start: usize) -> usize {
    let mut word_start = start;
    let mut in_word = false;

    for (idx, _char) in content[..start].char_indices().rev() {
        if !is_word_char(_char) {
            // The separators right before `start` are skipped first; the next
            // separator marks the word boundary.
            if in_word {
                break
            }

            continue
        }

        in_word = true;
        word_start = idx;
    }

    // Only separators before `start`: the start of the content is the nearest
    // boundary.
    if in_word {
        word_start
    } else {
        0
    }
}

/// Move the cursor to the end of the word at or after it, or to the end of the
/// content when no word is left.
pub fn cursor_next_word_end(content: &str, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(content, *cursor) else {
        return
    };

    *cursor = cursor_from_insertion_point(content, next_word_end(content, pos));
}

/// Move the cursor to the start of the word before it, or to the start of the
/// content when no word is left.
pub fn cursor_prev_word_start(content: &str, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(content, *cursor) else {
        return
    };

    *cursor = cursor_from_insertion_point(content, prev_word_start(content, pos));
}

/// Delete the word before the cursor and update the content and the cursor.
///
/// The range starts at [`prev_word_start`], so the separators between the word
/// and the cursor go as well: `:rename foo.txt` becomes `:rename foo.` when the
/// cursor follows `txt`.
///
/// Unlike [`delete_word`], the boundaries match word-wise movement, and the
/// content is never cut inside a multi-byte character.
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

/// Delete the word after the cursor and update the content and the cursor.
///
/// The range ends at [`next_word_end`], so the separators between the cursor
/// and the word go as well: `:rename foo.txt` becomes `:rename .txt` when the
/// cursor is before `foo`.
///
/// The boundaries match word-wise movement, and the content is never cut
/// inside a multi-byte character.
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

/// Delete the character after the cursor and update the cursor position.
///
/// The insertion point does not move, so repeating the operation deletes the
/// following characters one by one; a multi-byte character is removed whole.
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

/// Delete everything from the cursor to the end of the content (`Ctrl-k`).
///
/// The cursor stays at the same insertion point, which becomes the end of the
/// content; with no cursor nothing happens.
pub fn delete_to_end(_str: &mut String, cursor: &mut CursorPos) {
    let Some(pos) = cursor_insertion_point(_str, *cursor) else {
        return
    };

    _str.truncate(pos);
    *cursor = cursor_from_insertion_point(_str, pos);
}

/// The width `text` occupies when rendered in the terminal.
///
/// Wide characters (CJK) take two columns and combining marks none, so neither
/// the byte length nor the character count can lay out the completion popup or
/// the command-line cursor.
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

pub fn update_window_width(width: u16) {
    let current = WINDOW_WIDTH.load(Ordering::Acquire);
    if current != width {
        WINDOW_WIDTH.store(width, Ordering::Release);
    }
}

pub fn get_window_width() -> u16 {
    WINDOW_WIDTH.load(Ordering::Acquire)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_word_char() {
        assert!(is_word_char('a'));
        assert!(is_word_char('0'));
        assert!(is_word_char('文'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('/'));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('-'));
        // `_` separates words as well, so `foo_bar` is three words.
        assert!(!is_word_char('_'));
    }

    #[test]
    fn test_next_word_end() {
        let content = "hello world";

        assert_eq!(next_word_end(content, 0), 5);
        assert_eq!(next_word_end(content, 2), 5);

        assert_eq!(next_word_end(content, 5), 11);
        assert_eq!(next_word_end(content, 6), 11);
        assert_eq!(next_word_end(content, 7), 11);

        assert_eq!(next_word_end(content, 11), 11);
        assert_eq!(next_word_end("hello  ", 5), 7);

        let content = "foo_bar";
        assert_eq!(next_word_end(content, 0), 3);
        assert_eq!(next_word_end(content, 3), 7);
        assert_eq!(next_word_end(content, 7), 7);
    }

    #[test]
    fn test_prev_word_start() {
        let content = "hello world";

        assert_eq!(prev_word_start(content, 11), 6);
        assert_eq!(prev_word_start(content, 8), 6);

        assert_eq!(prev_word_start(content, 6), 0);
        assert_eq!(prev_word_start(content, 5), 0);

        assert_eq!(prev_word_start(content, 0), 0);
        assert_eq!(prev_word_start("  hello", 2), 0);

        let content = "foo_bar";
        assert_eq!(prev_word_start(content, 7), 4);
        assert_eq!(prev_word_start(content, 4), 0);
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

        // `_` is a word boundary for the movement as well.
        let content = "foo_bar";
        let mut cursor = CursorPos::Index(0);

        cursor_next_word_end(content, &mut cursor);
        assert!(cursor == CursorPos::Index(3));

        cursor_next_word_end(content, &mut cursor);
        assert!(cursor == CursorPos::End);

        cursor_prev_word_start(content, &mut cursor);
        assert!(cursor == CursorPos::Index(4));

        let mut cursor = CursorPos::None;
        cursor_next_word_end(content, &mut cursor);
        cursor_prev_word_start(content, &mut cursor);
        assert!(cursor == CursorPos::None);
    }

    #[test]
    fn test_delete_word_before_cursor() {
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

        let mut content = String::from("abc");
        let mut cursor = CursorPos::Index(0);

        delete_word_before_cursor(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::Index(0));

        // `_` is a word separator, so only the word after it is deleted.
        let mut content = String::from("foo_bar");
        let mut cursor = CursorPos::End;

        delete_word_before_cursor(&mut content, &mut cursor);
        assert_eq!(content, "foo_");
        assert!(cursor == CursorPos::End);
    }

    #[test]
    fn test_delete_char_after_cursor() {
        let mut content = String::from("abc");
        let mut cursor = CursorPos::Index(1);

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "ac");
        assert!(cursor == CursorPos::Index(1));

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "a");
        assert!(cursor == CursorPos::End);

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "a");
        assert!(cursor == CursorPos::End);

        // A multi-byte character is removed as a whole.
        let mut content = String::from("文件abc");
        let mut cursor = CursorPos::Index(0);

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "件abc");
        assert!(cursor == CursorPos::Index(0));

        let mut content = String::from("abc");
        let mut cursor = CursorPos::None;

        delete_char_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::None);
    }

    #[test]
    fn test_delete_word_after_cursor() {
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(2);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "he world");
        assert!(cursor == CursorPos::Index(2));

        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(5);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "hello");
        assert!(cursor == CursorPos::End);

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

        // `_` is a word separator, so only the word before it is deleted.
        let mut content = String::from("foo_bar");
        let mut cursor = CursorPos::Index(0);

        delete_word_after_cursor(&mut content, &mut cursor);
        assert_eq!(content, "_bar");
        assert!(cursor == CursorPos::Index(0));

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

        // `_` is an ASCII punctuation character, so it is a separator here too.
        let mut content = String::from("foo_bar");
        let mut cursor = CursorPos::End;
        delete_word(&mut content, &mut cursor);
        assert_eq!(content, "foo_");
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

    #[test]
    fn test_delete_to_end() {
        let mut content = String::from("hello world");
        let mut cursor = CursorPos::Index(5);

        delete_to_end(&mut content, &mut cursor);
        assert_eq!(content, "hello");
        assert!(cursor == CursorPos::End);

        let mut content = String::from("hello");
        let mut cursor = CursorPos::Index(0);

        delete_to_end(&mut content, &mut cursor);
        assert_eq!(content, "");
        assert!(cursor == CursorPos::End);

        let mut content = String::from("hello");
        let mut cursor = CursorPos::End;

        delete_to_end(&mut content, &mut cursor);
        assert_eq!(content, "hello");
        assert!(cursor == CursorPos::End);

        // Multi-byte characters are never cut.
        let mut content = String::from("文件abc");
        let mut cursor = CursorPos::Index(6);

        delete_to_end(&mut content, &mut cursor);
        assert_eq!(content, "文件");
        assert!(cursor == CursorPos::End);

        let mut content = String::from("abc");
        let mut cursor = CursorPos::None;

        delete_to_end(&mut content, &mut cursor);
        assert_eq!(content, "abc");
        assert!(cursor == CursorPos::None);
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-utils-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The preview calls a file it cannot decode as UTF-8 a "Non Text File";
    /// `l` uses the same test to choose an editor or the desktop handler.
    #[test]
    fn test_is_text_file() {
        let dir = temp_dir("text");

        let text = dir.join("text.txt");
        std::fs::write(&text, "hello\n世界\n").unwrap();
        assert!(is_text_file(&text));

        let empty = dir.join("empty");
        std::fs::write(&empty, "").unwrap();
        assert!(is_text_file(&empty));

        // Invalid UTF-8 is what `read_probe` rejects.
        let binary = dir.join("binary.bin");
        std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        assert!(!is_text_file(&binary));

        assert!(!is_text_file(&dir.join("missing")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `--shell` combines with the start directory, so `--start-from <path>
    /// --shell` opens the shell there.
    #[test]
    fn test_shell_flag() {
        let args = Args::try_parse_from(["hire", "-S"]).unwrap();
        assert!(args.shell);

        let args = Args::try_parse_from(["hire", "--shell"]).unwrap();
        assert!(args.shell);

        let args = Args::try_parse_from(["hire"]).unwrap();
        assert!(!args.shell);

        let args = Args::try_parse_from([
            "hire",
            "--start-from",
            "/tmp",
            "--shell"
        ]).unwrap();
        assert!(args.shell);
        assert_eq!(args.start_from, "/tmp");

        // Both modes spawn a shell but disagree about the directory.
        assert!(
            Args::try_parse_from(["hire", "-S", "-w"]).is_err()
        );
    }

    /// `-v` is a boolean of its own: `main` prints the version and leaves
    /// before any config or terminal setup.
    #[test]
    fn test_version_flag() {
        let args = Args::try_parse_from(["hire", "-v"]).unwrap();
        assert!(args.version);

        let args = Args::try_parse_from(["hire", "--version"]).unwrap();
        assert!(args.version);

        let args = Args::try_parse_from(["hire"]).unwrap();
        assert!(!args.version);

        // Only the lowercase `-v` is bound; clap's own `-V` is not enabled.
        assert!(Args::try_parse_from(["hire", "-V"]).is_err());
    }

    /// A path `xdg-mime` cannot identify falls back to the old label.
    #[test]
    fn test_file_type_label_falls_back() {
        let dir = temp_dir("mime");
        assert_eq!(file_type_label(&dir.join("missing")), "Non Text File");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `extension_start` is shared by the command-line and edit-mode paths.
    #[test]
    fn test_extension_start() {
        assert_eq!(extension_start("foo.txt"), Some(3));
        assert_eq!(extension_start("foo.tar.gz"), Some(7));
        assert_eq!(extension_start("abc."), Some(3));

        // No extension: no dot, a leading dot only, or nothing.
        assert_eq!(extension_start("foo"), None);
        assert_eq!(extension_start(".bashrc"), None);
        assert_eq!(extension_start(""), None);
        assert_eq!(extension_start("."), None);

        // A hidden file may still have an extension.
        assert_eq!(extension_start(".config.toml"), Some(7));

        // The index is a byte offset, so a multi-byte suffix stays intact.
        assert_eq!(extension_start("中文.txt"), Some(6));
        assert_eq!(extension_start("中文.中文"), Some(6));
    }

    /// The file list and the preview share one matching rule.
    #[test]
    fn test_search_indexes() {
        let names = ["alpha.rs", "beta.txt", "Alpha.md"];

        assert_eq!(
            search_indexes("alpha", names.iter().copied(), false),
            vec![0, 2]
        );
        assert_eq!(
            search_indexes("", names.iter().copied(), false),
            vec![0, 1, 2]
        );

        // An exact search keeps the case and stops at the first equal name.
        assert_eq!(
            search_indexes("beta", names.iter().copied(), true),
            Vec::<usize>::new()
        );
        assert_eq!(
            search_indexes("beta.txt", names.iter().copied(), true),
            vec![1]
        );
    }
}

