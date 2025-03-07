// UI

mod child_block;

use std::borrow::Cow;
use std::ops::AddAssign;
use std::collections::HashMap;

use child_block::render_file;
use ratatui::{
    Frame,
    text::{Line, Span, Text},
    style::{Color, Style, Modifier, Stylize},
    layout::{Constraint, Direction, Layout, Alignment},
    widgets::{Block, List, ListItem, Borders, Paragraph},
};

use crate::App;
use crate::app::{
    self,
    FileSaver,
    CursorPos,
    TermColors,
    MarkedFiles,
    FileOperation,
    reverse_style,
};

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

    // Parent Block
    let parent_block = Block::default()
        .borders(Borders::ALL);
    let parent_items = render_list(
        app.parent_files.iter(),
        app.selected_item.parent_selected(),
        &app.term_colors,
        None,
        FileOperation::None
    );
    let parent_list = List::new(parent_items).block(parent_block);

    frame.render_widget(computer_info, title_layout[0]);
    frame.render_widget(item_num_info, title_layout[1]);
    frame.render_stateful_widget(
        parent_list,
        browser_layout[0],
        &mut app.selected_item.parent
    );
    
    // Child Block
    match app.selected_block {
        app::Block::Browser(true) => {
            if app.file_content.is_some() {
                render_file(frame, app, browser_layout[1])?;
                frame.render_widget(render_command_line(app), chunks[2]);
                return Ok(())
            }
        },
        _ => {
            if app.file_content.is_some() {
                render_file(frame, app, browser_layout[2])?;
            } else {
                let child_block = Block::default()
                    .borders(Borders::ALL);
                let child_items = render_list(
                    app.child_files.iter(),
                    app.selected_item.child_selected(),
                    &app.term_colors,
                    None,
                    FileOperation::None
                );
                let child_items = List::new(child_items).block(child_block);

                frame.render_stateful_widget(
                    child_items,
                    browser_layout[2],
                    &mut app.selected_item.child
                );
            }
        }
    }

    // Current Block
    // Move current block to here to make preparation for file content of parent file.
    let current_block = Block::default()
        .borders(Borders::ALL);
    let marked_items = if app.path.to_string_lossy() == "/" {
        let path = app.path
            .join(&app.get_file_saver().unwrap().name);
        app.marked_files.get(&path)
    } else {
        app.marked_files.get(&app.path)
    };
    let current_items = render_list(
        app.current_files.iter(),
        app.selected_item.current_selected(),
        &app.term_colors,
        marked_items,
        app.marked_operation
    );
    let current_list = List::new(current_items)
        .block(current_block);

    frame.render_stateful_widget(
        current_list,
        browser_layout[1],
        &mut app.selected_item.current
    );

    // Command Block
    frame.render_widget(render_command_line(app), chunks[2]);

    Ok(())
}

fn check_app_error(app: &mut App) {
    use crate::app::Block as SBlock;

    if !app.app_error.is_empty() {
        let err_msg = app.app_error.to_string();

        if let SBlock::CommandLine(ref mut _msg, ref mut _cursor) = app.selected_block {
            if *_cursor != CursorPos::None {
                app.command_history.push(_msg.to_owned());
                *_cursor = CursorPos::None;

                *_msg = err_msg;
            } else {
                _msg.push_str(&format!("\n{}", err_msg));
            }
        } else {
            app.selected_block = SBlock::CommandLine(err_msg, CursorPos::None);
        }

        app.command_error = true;
        app.app_error.clear();
    }
}

/// Create a list of ListItem
fn render_list<'a>(files: std::slice::Iter<'a, FileSaver>,
                   idx: Option<usize>,
                   colors: &TermColors,
                   marked_items: Option<&'a MarkedFiles>,
                   marked_operation: FileOperation
) -> Vec<ListItem<'a>>
{
    let mut temp_items: Vec<ListItem> = Vec::new();
    if files.len() == 0 {
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

/// Function used to generate Paragraph at command-line layout.
fn render_command_line<'a>(app: &App) -> Paragraph<'a> {
    use app::Block as AppBlock;

    let block = Block::default();

    let message = match app.selected_block {
        AppBlock::Browser(_) => {
            let _selected_file = app.get_file_saver();
            if let Some(selected_file) = _selected_file {
                if selected_file.cannot_read {
                    Line::styled("DENIED", Style::default().red())
                } else if selected_file.dangling_symlink {
                    Line::styled(
                        "DANGLING_SYMLINK",
                        app.term_colors.orphan_style
                    )
                } else {
                    Line::from(vec![
                        selected_file.permission_span(),
                        Span::raw(" "),
                        selected_file.modified_span(),
                        Span::raw(" "),
                        selected_file.size_span(),
                        Span::raw(" "),
                        selected_file.symlink_span(app.term_colors.symlink_style)
                    ])
                }
            } else {
                Line::raw("")
            }
        },
        AppBlock::CommandLine(ref input, cursor) => {
            Line::from(get_command_line_span_list(
                input.to_owned(),
                cursor,
                app.command_error || app.command_warning
            ))
        }
    };

    Paragraph::new(message).block(block)
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

fn get_command_line_span_list<'a, S>(command: S,
                                     cursor: CursorPos,
                                     eye_catching: bool
) -> Vec<Span<'a>>
where S: Into<Cow<'a, str>>
{
    let mut span_list: Vec<Span> = Vec::new();
    if let CursorPos::Index(idx) = cursor {
        let mut i = 0;
        for c in command.into().chars() {
            span_list.push(
                if i == idx {
                    Span::raw(String::from(c))
                        .fg(Color::Black)
                        .bg(Color::White)
                } else {
                    Span::raw(String::from(c))
                        .fg(Color::White)
                }
            );
            i += 1;
        }

        return span_list
    }

    span_list.push(Span::from(command).fg(if eye_catching {
        Color::Red
    } else {
        Color::White
    }));

    if let CursorPos::End = cursor {
        span_list.push(Span::from(" ").fg(Color::Black).bg(Color::White));
    }

    span_list
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
                app.selected_item.current_selected().unwrap() + 1,
                app.current_files.len()
            )
        }
    };

    info
}

/// Create Paragraph structure with different color.
///
/// Make the text red when it's an error message.
fn get_command_line_style<'a, S>(app: &App,
                                 content: S,
                                 cursor: CursorPos
) -> Paragraph<'a>
where S: Into<Cow<'a, str>>
{
    if let CursorPos::None = cursor {
        let temp = Paragraph::new(
            Text::raw(content)
        )
            .scroll(app.command_scroll.unwrap());

        if app.command_error {
            return temp.red()
        }

        temp
    } else {
        Paragraph::new(Line::from(get_command_line_span_list(
            content,
            cursor,
            app.command_error
        )))
            .scroll(app.command_scroll.unwrap())
    }
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
