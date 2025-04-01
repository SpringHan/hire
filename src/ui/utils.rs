// Utils

use std::collections::HashMap;

use ratatui::style::Styled;

use crate::{app::{FileSaver, MarkedFiles, TermColors}, key_event::{EditItem, EditMode}};

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
) -> Vec<Item<'a>>
{
    let mut temp_items: Vec<Item> = Vec::new();

    for (idx, item) in edit_ref.iter().enumerate() {
        temp_items.push(get_editing_item_color(
            item,
            files.get(idx),
            colors,
            edit_ref.is_marked(idx)
        ));
    }

    temp_items
}

fn get_editing_item_color<'a>(
    item: &'a EditItem,
    file: Option<&FileSaver>,
    colors: &TermColors,
    marked: bool
) -> Item<'a> {
    let mut temp_item = Item::new(item.name(), None);

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

    // TODO: Add cursor & item type display

    temp_item.marked(if marked {
        Some(colors.marked_style)
    } else {
        None
    })
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
        .marked(if marked {
            Some(colors.marked_style)
        } else {
            None
        })
}
