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
            Constraint::Percentage(50),
            Constraint::Percentage(25)
        ])
        .split(chunks[1]);

    // Parent Block
    let parent_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default());
    let mut parent_items = render_list(app.parent_files.iter());
    let parent_list = List::new(parent_items);

    // Current Block
    let current_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default());

    // Child Block
    let child_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default());

    frame.render_widget(parent_list, browser_layout[0]);
    frame.render_widget(current_block, browser_layout[1]);
    frame.render_widget(child_block, browser_layout[2]);
}

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
