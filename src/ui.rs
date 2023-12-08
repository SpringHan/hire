// UI

use crate::App;
use crate::app::{self, filesaver::FileSaver, CursorPos, MarkedFiles, FileOperation};

use std::ops::AddAssign;
use std::collections::HashMap;

use ratatui::{
    Frame,
    text::{Line, Span, Text},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier, Stylize},
    widgets::{Block, List, ListItem, Borders, Paragraph, Wrap}
};

pub fn ui(frame: &mut Frame, app: &mut App) {
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

    // Expanded Commandline
    if app.command_expand {
        let command_block = Block::default().on_black();
        if let app::Block::CommandLine(ref error, _) = app.selected_block {
            let command_errors = Paragraph::new(
                Text::raw(error)
            )
                .red()
                .wrap(Wrap { trim: true })
                .scroll((app.command_scroll.unwrap(), 0))
                .block(command_block);

            frame.render_widget(title_paragraph, chunks[0]);
            frame.render_widget(command_errors, chunks[1]);
            return ()
        } else {
            app.quit_command_mode();
        }
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
        .borders(Borders::ALL)
        .on_black();
    let parent_items = render_list(
        app.parent_files.iter(),
        app.selected_item.parent_selected(),
        None,
        FileOperation::None
    );
    let parent_list = List::new(parent_items).block(parent_block);

    frame.render_widget(title_paragraph, chunks[0]);
    frame.render_stateful_widget(
        parent_list,
        browser_layout[0],
        &mut app.selected_item.parent
    );
    
    // Child Block
    match app.selected_block {
        app::Block::Browser(true) => {
            if app.file_content.is_some() {
                frame.render_widget(render_file_content(app), browser_layout[1]);
                frame.render_widget(render_command_line(app), chunks[2]);
                return ()
            }
        },
        _ => {
            if app.file_content.is_some() {
                frame.render_widget(render_file_content(app),browser_layout[2]);
            } else {
                let child_block = Block::default()
                    .borders(Borders::ALL)
                    .on_black();
                let child_items = render_list(
                    app.child_files.iter(),
                    app.selected_item.child_selected(),
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
        .borders(Borders::ALL)
        .on_black();
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
}

/// Create a list of ListItem
fn render_list<'a>(files: std::slice::Iter<'a, FileSaver>,
                   idx: Option<usize>,
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
                            ListItem::new(Line::from(
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
                            ))
                                .bg(if *num == i {
                                    num.add_assign(1);
                                    Color::LightYellow
                                } else {
                                    num.add_assign(1);
                                    Color::Black
                                })
                        } else if *num == i {
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

/// Render current file content if the selected file is not a dir.
fn render_file_content(app: &App) -> Paragraph {
    let file_block = Block::default()
        .borders(Borders::ALL)
        .on_black();
    if let Some(ref content) = app.file_content {
        Paragraph::new(content.as_str()).block(file_block)
    } else {
        panic!("Unknown error!")
    }
}

/// Function used to generate Paragraph at command-line layout.
fn render_command_line(app: &App) -> Paragraph {
    use app::Block as ABlock;

    let block = Block::default().on_black();
    let message = match app.selected_block {
        ABlock::Browser(_) => {
            let selected_file = app.get_file_saver();
            if let Some(selected_file) = selected_file {
                if selected_file.cannot_read {
                    Line::styled("DENIED", Style::default().red())
                } else if selected_file.dangling_symlink {
                    Line::styled(
                        "DANGLING_SYMLINK",
                        Style::default().light_red()
                    )
                } else {
                    Line::from(vec![
                        selected_file.permission_span(),
                        Span::raw(" "),
                        selected_file.modified_span(),
                        Span::raw(" "),
                        selected_file.size_span(),
                        Span::raw(" "),
                        selected_file.symlink_span()
                    ])
                }
            } else {
                Line::raw("")
            }
        },
        ABlock::CommandLine(ref input, cursor) => {
            Line::from(get_command_line_span_list(
                input,
                cursor,
                app.command_error
            ))
        }
    };

    Paragraph::new(message).block(block)
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

fn get_command_line_span_list(
    command: &String,
    cursor: CursorPos,
    error_msg: bool
) -> Vec<Span>
{
    let mut span_list: Vec<Span> = Vec::new();
    if let CursorPos::Index(idx) = cursor {
        let mut i = 0;
        for c in command.chars() {
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

    span_list.push(Span::from(command).fg(if error_msg {
        Color::Red
    } else {
        Color::White
    }));
    span_list.push(Span::from(" ").fg(Color::Black).bg(Color::White));

    span_list
}
