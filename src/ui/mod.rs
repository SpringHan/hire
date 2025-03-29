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

use crate::{app::CmdContent, App};
use crate::app::{self, CursorPos};

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
    let title_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75),
            Constraint::Percentage(25)
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
    let item_num_info = Paragraph::new(
        Line::from(
            Span::styled(get_item_num_para(app), app.term_colors.file_style)
        )
    )
        .alignment(Alignment::Right)
        .block(item_info_block);

    // Expanded Commandline
    if app.command_expand {
        let command_block = Block::default();
        // TODO: Add other command style.
        if let app::Block::CommandLine(ref msg, cursor) = app.selected_block {
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
    let constraints = match app.selected_block {
        app::Block::Browser(true) => {
            vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50)
            ]
        },
        _ => {
            vec![
                Constraint::Percentage(25),
                Constraint::Percentage(30),
                Constraint::Percentage(45)
            ]
        }
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
        app::Block::Browser(true) => {
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
    use crate::app::Block as SBlock;

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

fn get_item_num_para(app: &App) -> String {
    let info = if app.path.to_string_lossy() == "/" {
        format!(
            "{}/{}",
            app.selected_item.parent_selected().unwrap() + 1,
            app.parent_files.len()
        )
    } else {
        if app.current_files.is_empty() {
            String::new()
        } else {
            format!(
                "{}/{}",
                // TODO: Rewrite without unwrap.
                app.selected_item.current_selected().unwrap() + 1,
                app.current_files.len()
            )
        }
    };

    info
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
