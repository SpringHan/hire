// Utils

use std::collections::HashMap;

use ratatui::style::Styled;

use crate::app::{MarkedFiles, FileSaver, TermColors};

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

/// Return the item which has the style of normal file.
fn get_normal_item_color<'a>(file: &'a FileSaver,
                             colors: &TermColors,
                             marked: bool
                             // reverse: bool
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
