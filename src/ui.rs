// UI

use crate::App;
use crate::app::{self, filesaver::FileSaver};

use std::ops::AddAssign;

use ratatui::{
    Frame,
    text::{Line, Span},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier, Stylize},
    widgets::{Block, List, ListItem, Borders, Paragraph}
};

pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(4),
            Constraint::Percentage(93),
            Constraint::Percentage(2)
        ])
        .split(frame.size());

    // Title
    let title_block = Block::default().on_black();
    let title_paragraph = Paragraph::new(
        Line::from(
            vec![
                Span::raw(format!("{}@{}", app.user_name, app.computer_name))
                    .light_green()
                    .bold(),
                Span::raw(format!("  {}", app.path.to_string_lossy()))
                    .white()
                    .bold()
            ]))
        .block(title_block);


    // File browser layout
    let browser_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Percentage(45)
        ])
        .split(chunks[1]);

    // Parent Block
    let parent_block = Block::default()
        .borders(Borders::ALL)
        .on_black();
    let parent_items = render_list(
        app.parent_files.iter(),
        Some(app.selected_item.0)
    );
    let parent_list = List::new(parent_items).block(parent_block);

    // Current Block
    let current_block = Block::default()
        .borders(Borders::ALL)
        .on_black();
    let current_items = render_list(
        app.current_files.iter(),
        Some(app.selected_item.1)
    );
    let current_list = List::new(current_items).block(current_block);

    // Child Block
    let child_block = Block::default()
        .borders(Borders::ALL)
        .on_black();
    let child_items = render_list(
        app.child_files.iter(),
        None
    );
    let child_list = List::new(child_items).block(child_block);

    frame.render_widget(title_paragraph, chunks[0]);
    frame.render_widget(parent_list, browser_layout[0]);
    frame.render_widget(current_list, browser_layout[1]);
    frame.render_widget(child_list, browser_layout[2]);
    // Command Block
    // render_command_line(app);
}

/// Create a list of ListItem
fn render_list(files: std::slice::Iter<'_, FileSaver>, idx: Option<usize>) -> Vec<ListItem> {
    let mut temp_items: Vec<ListItem> = Vec::new();
    if files.len() == 0 {
        return temp_items
    }

    let mut current_item: Option<usize> =  if let Some(_) = idx {
        Some(0)
    } else {
        None
    };

    for file in files {
        temp_items.push(
            if let Some(ref mut num) = current_item {
                match idx {
                    Some(i) => {
                        // Make the style of selected item
                        if *num == i {
                            num.add_assign(1);
                            ListItem::new(Line::from(Span::styled(
                                &file.name,
                                Style::default()
                                    .fg(Color::Black)
                                    .add_modifier(get_file_font_style(file.is_dir))
                            )))
                                .on_white()
                        } else {
                            num.add_assign(1);
                            get_normal_item_color(file)
                        }
                    },
                    None => panic!("Unknow error!")
                }
            } else {
                get_normal_item_color(file)
            }
        );
    }

    temp_items
}

fn render_command_line(app: &App) -> Paragraph {
    let block = Block::default().on_black();
    match app.selected_block {
        app::Block::Browser => {
        },
        app::Block::CommandLine => {
            
        },
    }

    todo!()
}

/// Return the item which has the style of normal file.
fn get_normal_item_color(file: &FileSaver) -> ListItem {
    ListItem::new(
        Line::from(
            Span::styled(
                &file.name,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(get_file_font_style(file.is_dir))
            )
        )
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
