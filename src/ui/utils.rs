// Utils

use std::collections::HashMap;
use std::path::Path;

use ratatui::{
    style::{Color, Modifier, Style, Styled, Stylize},
    text::{Line, Span},
};

use crate::{
    key_event::{EditItem, EditMode},
    utils::{CursorPos, MarkedFiles},
    app::{FileSaver, TermColors},
};

use super::list::Item;

/// Create a list of ListItem
pub fn render_list<'a>(
    files: std::slice::Iter<'a, FileSaver>,
    colors: &TermColors,
    marked_items: Option<&'a MarkedFiles>,
) -> (Vec<Item<'a>>, bool)
{
    let mut temp_items: Vec<Item> = Vec::new();
    if files.len() == 0 {
        return (temp_items, false)
    }

    // Use this method to avoid extra clone.
    let mut marked = false;
    let temp_set: HashMap<String, bool> = HashMap::new();
    let marked_files = if let Some(item) = marked_items {
        &item.files
    } else {
        &temp_set
    };

    for file in files {
        temp_items.push(get_normal_item_color(
            file,
            colors,
            if marked_files.contains_key(&file.name) {
                if !marked {
                    marked = true;
                }

                true
            } else {
                false
            }
        ));
    }

    (temp_items, marked)
}

/// Create ListItems for EditItems
pub fn render_editing_list<'a>(
    edit_ref: &'a EditMode,
    files: &Vec<FileSaver>,
    colors: &TermColors
) -> (Vec<Item<'a>>, bool)
{
    let mut sidebar_used = false;
    let mut temp_items: Vec<Item> = Vec::new();

    for (idx, item) in edit_ref.iter().enumerate() {
        let (item, style) = get_editing_item_color(
            item,
            files.get(idx),
            colors,
            edit_ref.is_marked(idx)
        );

        if style.is_some() && !sidebar_used {
            sidebar_used = true;
        }

        temp_items.push(item.sidebar(style));
    }

    (temp_items, sidebar_used)
}

/// Build the line of an editing name with the character under the cursor
/// highlighted.
///
/// The cursor is a byte offset on a character boundary, so the name is walked
/// with `char_indices()` and compared by the byte position. Comparing the
/// character count would be wrong as soon as the name holds multi-byte
/// characters (e.g. CJK ones).
fn editing_name_line<'a>(name: &'a str, cursor: CursorPos) -> Line<'a> {
    if let CursorPos::Index(pos) = cursor {
        let mut line = Line::default();
        let mut cursor_drawn = false;

        for (byte_start, _char) in name.char_indices() {
            let char_len = _char.len_utf8();

            // A cursor placed inside a character (which shouldn't happen, but
            // is handled gracefully) belongs to that character.
            if pos >= byte_start && pos < byte_start + char_len {
                cursor_drawn = true;
                line.push_span(
                    Span::raw(_char.to_string())
                        .fg(Color::Black)
                        .bg(Color::White)
                );
            } else {
                line.push_span(Span::raw(_char.to_string()));
            }
        }

        // The cursor may be placed at the end of the name, and it has to stay
        // visible in that case.
        if !cursor_drawn {
            line.push_span(Span::raw(" ").fg(Color::Black).bg(Color::White));
        }

        return line
    }

    if let CursorPos::End = cursor {
        let mut line = Line::raw(name);
        line.push_span(Span::raw(" ").bg(Color::White));
        return line
    }

    Line::raw(name)
}

fn get_editing_item_color<'a>(
    item: &'a EditItem,
    file: Option<&FileSaver>,
    colors: &TermColors,
    marked: bool,
) -> (Item<'a>, Option<Style>) {
    let left_text = editing_name_line(item.name(), item.cursor());

    let mut temp_item = Item::new(left_text, None);

    if let Some(_file) = file {
        let style = if _file.is_dir {
            colors.dir_style
        } else if _file.dangling_symlink {
            colors.orphan_style
        } else if _file.executable {
            colors.executable_style
        } else if _file.symlink_file.is_some() {
            colors.symlink_style
        } else {
            colors.file_style
        };

        temp_item = temp_item.set_style(style);
    } else if item.is_dir {
        temp_item = temp_item.set_style(colors.dir_style);
    }

    let sidebar_style = if marked {
        Some(colors.marked_style.add_modifier(Modifier::REVERSED))
    } else if item.delete() {
        Some(colors.orphan_style
             .add_modifier(Modifier::REVERSED))
    } else {
        None
    };

    (temp_item, sidebar_style)
}

/// Return the item which has the style of normal file.
fn get_normal_item_color<'a>(
    file: &'a FileSaver,
    colors: &TermColors,
    marked: bool
) -> Item<'a>
{
    let style = if file.is_dir {
        colors.dir_style
    } else if file.dangling_symlink {
        colors.orphan_style
    } else if file.executable {
        colors.executable_style
    } else if file.symlink_file.is_some() {
        colors.symlink_style
    } else {
        colors.file_style
    };

    Item::new::<&str>(&file.name, None).set_style(style)
        .sidebar(if marked {
            Some(colors.marked_style.add_modifier(Modifier::REVERSED))
        } else {
            None
        })
}

/// Shorten a path so that it can be displayed in a single line.
///
/// The home directory is displayed as `~`; when the path is too long, every
/// directory component before the last one is abbreviated to its first
/// character, and a leading `…` is used if it's still longer than `max_len`.
pub fn shorten_path(
    path: &Path,
    home: Option<&Path>,
    max_len: usize
) -> String {
    let display = match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => {
            if rest.as_os_str().is_empty() {
                String::from("~")
            } else {
                format!("~/{}", rest.to_string_lossy())
            }
        },
        None => path.to_string_lossy().into_owned(),
    };

    if display.chars().count() <= max_len {
        return display
    }

    // Abbreviate every directory component except the last one, which is the
    // name of the directory itself.
    let components: Vec<&str> = display.split('/').collect();
    let last = components.len().saturating_sub(1);
    let mut shorted: Vec<String> = Vec::with_capacity(components.len());

    for (idx, component) in components.iter().enumerate() {
        if idx < last && component.chars().count() > 1 {
            shorted.push(component.chars().next().unwrap().to_string());
        } else {
            shorted.push((*component).to_owned());
        }
    }

    let shorted = shorted.join("/");
    if shorted.chars().count() <= max_len {
        return shorted
    }

    // Keep the tail of the path when it's still too long.
    let mut tail: Vec<char> = shorted.chars()
        .rev()
        .take(max_len.saturating_sub(1))
        .collect();
    tail.reverse();

    format!("…{}", tail.into_iter().collect::<String>())
}

#[cfg(test)]
mod test {
    use super::*;

    /// Return the content of the spans which are drawn as the cursor.
    fn highlighted(spans: &[Span]) -> Vec<String> {
        spans
            .iter()
            .filter(|span| span.style.bg == Some(Color::White))
            .map(|span| span.content.to_string())
            .collect()
    }

    #[test]
    fn test_editing_name_cursor_with_multibyte_content() {
        let line = editing_name_line("中文.txt", CursorPos::Index(0));
        assert_eq!(highlighted(&line.spans), vec![String::from("中")]);

        let line = editing_name_line("中文.txt", CursorPos::Index(3));
        assert_eq!(highlighted(&line.spans), vec![String::from("文")]);

        let line = editing_name_line("中文.txt", CursorPos::End);
        assert_eq!(highlighted(&line.spans), vec![String::from(" ")]);

        // A cursor at the end of the name stays visible.
        let line = editing_name_line("中文.txt", CursorPos::Index(10));
        assert_eq!(highlighted(&line.spans), vec![String::from(" ")]);
    }

    #[test]
    fn test_editing_name_cursor_with_ascii_content() {
        let line = editing_name_line("abc.txt", CursorPos::Index(1));
        assert_eq!(highlighted(&line.spans), vec![String::from("b")]);
    }

    #[test]
    fn test_shorten_path() {
        let home = Path::new("/home/user");

        assert_eq!(
            shorten_path(Path::new("/home/user/dev/hire"), Some(home), 32),
            "~/dev/hire"
        );
        assert_eq!(shorten_path(home, Some(home), 32), "~");
        assert_eq!(shorten_path(Path::new("/"), Some(home), 32), "/");
        assert_eq!(
            shorten_path(
                Path::new("/home/user/a/bb/cc/dd/ee/ff"),
                Some(home),
                15
            ),
            "~/a/b/c/d/e/ff"
        );

        // Only the last component is kept complete, so the ellipsis mode is
        // used when the abbreviated path is still too long.
        let long = shorten_path(
            Path::new("/aaa/bbb/ccc/ddd/eee/fff/ggg/hhh/iii/last-component"),
            None,
            10
        );
        assert_eq!(long.chars().count(), 10);
        assert!(long.starts_with("…"));
        assert_eq!(long, "…component");
    }
}
