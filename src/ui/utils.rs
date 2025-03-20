// Utils

use std::{collections::HashMap, ops::AddAssign};

use ratatui::{
    style::{Color, Modifier, Stylize},
    text::{Line, Span},
    widgets::ListItem
};

use crate::app::{
    reverse_style,
    FileOperation,
    MarkedFiles,
    FileSaver,
    TermColors
};

/// Create a list of ListItem
pub fn render_list<'a>(files: std::slice::Iter<'a, FileSaver>,
                   idx: Option<usize>,
                   colors: &TermColors,
                   marked_items: Option<&'a MarkedFiles>,
                   marked_operation: FileOperation
) -> Vec<ListItem<'a>>
{
    let mut temp_items: Vec<ListItem> = Vec::new();
    if files.len() == 0 {
        temp_items.push(ListItem::new("Empty").fg(Color::Red));

        return temp_items
    }

    let mut current_item: Option<usize> =  if let Some(_) = idx {
        Some(0)
    } else {
        None
    };

    // Use this method to avoid extra clone.
    let temp_set: HashMap<String, bool> = HashMap::new();
    let mut to_be_moved = false;
    let marked_files = if let Some(item) = marked_items {
        if marked_operation == FileOperation::Move {
            to_be_moved = true;
        }
        &item.files
    } else {
        &temp_set
    };

    // TODO: Refactor this lines.
    for file in files {
        temp_items.push(
            if let Some(ref mut num) = current_item {
                match idx {
                    Some(i) => {
                        // Make the style of selected item
                        if marked_files.contains_key(&file.name) {
                            let item = ListItem::new(Line::from(
                                Span::raw(&file.name)
                                    .fg(if *num == i {
                                        Color::Black
                                    } else {
                                        Color::LightYellow
                                    })
                                    .add_modifier(get_file_font_style(file.is_dir))
                                    .add_modifier(if to_be_moved {
                                        Modifier::ITALIC
                                    } else {
                                        Modifier::empty()
                                    })
                            ));
                            if *num == i {
                                num.add_assign(1);
                                item.bg(Color::LightYellow)
                            } else {
                                num.add_assign(1);
                                item
                            }
                        } else if *num == i {
                            num.add_assign(1);
                            get_normal_item_color(file, colors, true)
                        } else {
                            num.add_assign(1);
                            get_normal_item_color(file, colors, false)
                        }
                    },
                    None => panic!("Unknow error when rendering list!")
                }
            } else {
                get_normal_item_color(file, colors, false)
            }
        );
    }

    temp_items
}

/// Return the item which has the style of normal file.
fn get_normal_item_color<'a>(file: &'a FileSaver,
                             colors: &TermColors,
                             reverse: bool
) -> ListItem<'a>
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

    ListItem::new(Line::raw(&file.name)).style(
        if reverse {
            reverse_style(style)
        } else {
            style
        }
    )
}

/// Return bold if the file is a directory.
/// Otherwise return undefined.
fn get_file_font_style(is_dir: bool) -> Modifier {
    if is_dir {
        Modifier::BOLD
    } else {
        Modifier::empty()
    }
}
