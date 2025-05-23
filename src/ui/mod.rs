// UI

mod list;
mod utils;
mod child_block;
mod command_line;
mod parent_block;
mod current_block;
mod cmdline_popup;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Paragraph},
    style::{Modifier, Stylize},
    text::{Line, Span},
    Frame
};

use crate::{app::App, utils::{self as cutils, CursorPos, CmdContent}};

use command_line::*;
use parent_block::render_parent;
use current_block::render_current;
use cmdline_popup::render_completion;
use child_block::{render_child, render_file};

pub use child_block::update_file_linenr;

pub fn ui(frame: &mut Frame, app: &mut App) -> anyhow::Result<()> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if !app.command_expand {
            vec![
                Constraint::Percentage(4),
                Constraint::Percentage(93),
                Constraint::Percentage(2)
            ]
        } else {
            vec![
                Constraint::Percentage(4),
                Constraint::Percentage(96)
            ]
        })
        .split(frame.area());

    // Title
    let (title_right, right_length) = get_title_right_info(app);
    let title_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(100),
            Constraint::Min(right_length as u16)
        ])
        .split(chunks[0]);
    
    let computer_info_block = Block::default();
    let computer_info = Paragraph::new(
        Line::from(
            vec![
                Span::styled(
                    format!("{}@{}", app.user_name, app.computer_name),
                    app.term_colors.executable_style
                ),
                Span::styled(
                    format!(
                        "  {}{}",
                        short_display_path(app),
                        // Show a slash when needed.
                        if app.path.to_string_lossy() == "/" {
                            ""
                        } else {
                            "/"
                        }
                    ),
                    app.term_colors.dir_style
                ),
                {
                    let current_file = app.get_file_saver();
                    let file = if let Some(current_file) = current_file {
                        current_file.name.to_owned()
                    } else {
                        String::new()
                    };
                    Span::styled(file, app.term_colors.file_style)
                        .add_modifier(Modifier::BOLD)
                }
            ]))
        .block(computer_info_block);

    // App Error
    check_app_error(app);

    // Item list statistic information
    let item_info_block = Block::default();
    let item_num_info = title_right
        .alignment(Alignment::Right)
        .block(item_info_block);

    // Expanded Commandline
    if app.command_expand {
        let command_block = Block::default();
        if let cutils::Block::CommandLine(ref msg, cursor) = app.selected_block {
            let command_errors = get_command_line_style(
                app,
                msg,
                cursor
            ).block(command_block);

            frame.render_widget(computer_info, chunks[0]);
            frame.render_widget(command_errors, chunks[1]);
            render_completion(app, frame, chunks[1]);
            return Ok(())
        }

        app.quit_command_mode();
    }


    // File browser layout
    let constraints = if let cutils::Block::Browser(true) = app.selected_block {
        vec![
            Constraint::Percentage(50),
            Constraint::Percentage(50)
        ]
    } else if app.edit_mode.enabled {
        vec![
            Constraint::Percentage(20),
            Constraint::Percentage(55),
            Constraint::Percentage(25)
        ]
    } else {
        vec![
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Percentage(45)
        ]
    };

    let browser_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(chunks[1]);

    // Title layer
    frame.render_widget(computer_info, title_layout[0]);
    frame.render_widget(item_num_info, title_layout[1]);

    // Parent block
    render_parent(app, frame, browser_layout[0]);
    
    // Child Block
    match app.selected_block {
        cutils::Block::Browser(true) => {
            if app.file_content.is_some() {
                render_file(frame, app, browser_layout[1])?;
                render_command_line(app, frame, chunks[2]);
                return Ok(())
            }
        },
        _ => {
            if app.file_content.is_some() {
                render_file(frame, app, browser_layout[2])?;
            } else {
                render_child(app, frame, browser_layout[2]);
            }
        }
    }

    // Current Block
    render_current(app, frame, browser_layout[1]);

    // Command Block
    render_command_line(app, frame, chunks[2]);
    render_completion(app, frame, chunks[2]);

    Ok(())
}

fn check_app_error(app: &mut App) {
    use cutils::Block as SBlock;

    if !app.app_error.is_empty() {
        let err_msg = app.app_error.to_string();

        if let SBlock::CommandLine(ref mut _msg, ref mut _cursor) = app.selected_block {
            if *_cursor != CursorPos::None {
                app.command_history.push(_msg.get_str().to_owned());
                *_cursor = CursorPos::None;

                *_msg = CmdContent::String(err_msg);
            } else {
                match *_msg {
                    CmdContent::String(ref mut messages) => {
                        messages.push_str(&format!("\n{}", err_msg));
                    },
                    CmdContent::Text(ref mut text) => {
                        text.push_line(err_msg);
                    },
                }
            }
        } else {
            app.selected_block = SBlock::CommandLine(
                CmdContent::String(err_msg),
                CursorPos::None
            );
        }

        app.command_error = true;
        app.app_error.clear();
    }
}

#[inline(always)]
fn get_title_right_info(app: &App) -> (Paragraph<'static>, usize) {
    let item_index = if app.path.to_string_lossy() == "/" {
        format!(
            "{}/{}",
            app.selected_item.parent_selected().unwrap() + 1,
            app.parent_files.len()
        )
    } else {
        let current_idx = app.selected_item.current.selected();
        if app.current_files.is_empty() || current_idx.is_none() {
            String::new()
        } else {
            format!(
                "{}/{}",
                current_idx.unwrap() + 1,
                app.current_files.len()
            )
        }
    };

    let tab_index = format!(
        "|{}{} ",
        app.tab_list.current() + 1,
        if app.tab_list.current() == app.tab_list.len() - 1 {
            "|"
        } else {
            ">"
        }
    );

    let length = item_index.len() + tab_index.len();
    let mut line = Line::default();
    line.push_span(Span::raw(tab_index).bold());
    line.push_span(Span::raw(item_index));

    (Paragraph::new(line), length)
}

fn short_display_path(app: &App) -> String {
    let path = app.path.to_string_lossy();
    let file = if let Some(file_saver) = app.get_file_saver() {
        &file_saver.name
    } else {
        ""
    };

    if path.len() + file.len() <= 68 {
        return path.into()
    }

    let mut splited_path: Vec<&str> = path.split("/").collect();
    splited_path.remove(0);     // Remove the empty string.

    for i in 0..(splited_path.len() - 1) {
        splited_path[i] = splited_path[i].get(0..1).unwrap();
    }

    let shorted_path = splited_path.join("/");

    format!("/{}", shorted_path)
}
