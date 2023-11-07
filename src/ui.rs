// UI

use crate::App;

use ratatui::{
    Frame,
    text::{Text, Line, Span},
    layout::{Rect, Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    widgets::{Block, List, ListItem, Paragraph, Borders}
};

pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(3),
            Constraint::Percentage(97)
        ])
        .split(frame.size());

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
        .style(Style::default());
    let parent_items = render_list(app.parent_files.iter());
    let parent_list = List::new(parent_items).block(parent_block);

    // Current Block
    let current_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default());
    let current_items = render_list(app.current_files.iter());
    let current_list = List::new(current_items).block(current_block);

    // Child Block
    let child_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default());
    let child_items = render_list(app.child_files.iter());
    let child_list = List::new(child_items).block(child_block);

    frame.render_widget(parent_list, browser_layout[0]);
    frame.render_widget(current_list, browser_layout[1]);
    frame.render_widget(child_list, browser_layout[2]);
}

/// Create a list of ListItem
fn render_list(files: std::slice::Iter<'_, String>) -> Vec<ListItem> {
    let mut temp_items: Vec<ListItem> = Vec::new();
    if files.len() == 0 {
        return temp_items
    }

    for file in files {
        temp_items.push(
            ListItem::new(Line::from(
                Span::styled(file, Style::default()
                             .fg(Color::White))
            ))
        );
    }

    temp_items
}
