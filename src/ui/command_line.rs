// Command line

use std::borrow::Cow;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    widgets::{Block, Paragraph, Widget},
    text::{Line, Span, Text},
    Frame
};

use crate::app::{self, App, CursorPos};

/// The widget to show states, such as file permission, size, etc.
pub struct StateLine<'a> {
    left_side: Line<'a>,
    right_side: Option<Line<'a>>
}

impl<'a> StateLine<'a> {
    // NOTE: There's no possibility that the length of lines doesn't equal to 2.
    pub fn new(lines: Vec<Line<'a>>) -> Self {
        let right_side = if lines.len() < 2 {
            None
        } else {
            Some(lines[1].to_owned())
        };

        Self {
            left_side: lines[0].to_owned(),
            right_side
        }
    }
}

impl<'a> Widget for StateLine<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where Self: Sized
    {
        if self.right_side.is_none() {
            self.left_side.render(area, buf);
            return ()
        }

        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
                Constraint::Min(7)
            ])
            .split(area);

        self.left_side.render(layout[0], buf);
        self.right_side.unwrap().render(layout[1], buf);
    }
}

/// Function used to generate Paragraph at command-line layout.
pub fn render_command_line<'a>(
    app: &App,
    frame: &mut Frame,
    area: Rect
)
{
    use app::Block as AppBlock;

    match app.selected_block {
        AppBlock::Browser(_) => {
            let mut lines: Vec<Line> = Vec::new();

            let _selected_file = app.get_file_saver();
            if let Some(selected_file) = _selected_file {
                if selected_file.cannot_read {
                    lines.push(Line::styled(
                        "DENIED",
                        Style::default().red()
                    ).alignment(Alignment::Left));
                } else if selected_file.dangling_symlink {
                    lines.push(Line::styled(
                        "DANGLING_SYMLINK",
                        app.term_colors.orphan_style
                    ).alignment(Alignment::Left));
                } else {
                    lines.push(Line::from(vec![
                        selected_file.permission_span(),
                        Span::raw(" "),
                        selected_file.modified_span(),
                        Span::raw(" "),
                        selected_file.size_span(),
                        Span::raw(" "),
                        selected_file.symlink_span(app.term_colors.symlink_style)
                    ]).alignment(Alignment::Left));
                }
            } else {
                lines.push(Line::raw("").alignment(Alignment::Left));
            }

            if app.confirm_output && app.output_file.is_some() {
                lines.push(Line::styled(
                    "OUTPUT",
                    Style::default().cyan().add_modifier(Modifier::BOLD)
                ).alignment(Alignment::Right));
            }

            frame.render_widget(
                StateLine::new(lines),
                area
            );
        },
        AppBlock::CommandLine(ref input, cursor) => {
            let block = Block::default();
            let para = Paragraph::new(vec![Line::from(
                get_command_line_span_list(
                    input.to_owned(),
                    cursor,
                    app.command_error || app.command_warning
                )
            )]).block(block);

            frame.render_widget(para, area);
        }
    }

    // Paragraph::new(message).block(block)
}

/// Create Paragraph structure with different color.
///
/// Make the text red when it's an error message.
pub fn get_command_line_style<'a, S>(app: &App,
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
