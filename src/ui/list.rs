// List Widget

use ratatui::{
    widgets::{Block, ListState, StatefulWidget, Widget},
    style::{Color, Modifier, Style, Styled, Stylize},
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span, Text},
    buffer::Buffer,
};

use crate::utils::get_window_height;

pub struct Item<'a> {
    style: Style,
    left: Line<'a>,
    right: Option<Line<'a>>,
    marked_style: Option<Style>,
}

pub struct List<'a> {
    block: Block<'a>,
    items: Vec<Item<'a>>,

    // Navigation index
    navi_show: bool,
    navi_index: Option<usize>,

    marked: bool,
    index_color: Color,
}

impl<'a> Item<'a> {
    pub fn new<T>(left: T, right: Option<T>) -> Self
    where T: Into<Line<'a>>
    {
        let _right = if let Some(text) = right {
            Some(text.into())
        } else {
            None
        };

        Self {
            right: _right,
            left: left.into(),
            marked_style: None,
            style: Style::default(),
        }
    }

    pub fn marked(mut self, marked: Option<Style>) -> Self {
        self.marked_style = marked;
        self
    }
}

impl Styled for Item<'_> {
    type Item = Self;

    fn style(&self) -> Style {
        self.style
    }

    fn set_style<S: Into<Style>>(mut self, style: S) -> Self::Item {
        self.style = style.into();
        self
    }
}

impl Widget for Item<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, self.style);

        let right = if let Some(mut line) = self.right {
            if let Some(style) = self.marked_style {
                line.push_span(Span::raw(" ").style(style));
            }

            Some(line)
        } else {
            if let Some(style) = self.marked_style {
                Some(Line::raw(" ").style(style))
            } else {
                None
            }
        };

        if right.is_none() {
            self.left.render(area, buf);
            return ()
        }

        let right_line = right.unwrap();
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
                Constraint::Min(right_line.width() as u16)
            ])
            .split(area);

        self.left.render(layout[0], buf);
        right_line.render(layout[1], buf);
    }
}

impl<'a> List<'a> {
    pub fn new(block: Block<'a>, items: Vec<Item<'a>>, marked: bool) -> Self {
        Self {
            block,
            items,
            marked,
            navi_index: None,
            navi_show: false,
            index_color: Color::default()
        }
    }

    pub fn index(
        mut self,
        navi_show: bool,
        index: Option<usize>,
        style: Style
    ) -> Self
    {
        if !navi_show {
            return self
        }

        self.navi_show = true;
        self.navi_index = index;

        if let Some(color) = style.fg {
            self.index_color = color;
        }

        self
    }

    fn adjust_offset(state: &mut ListState) {
        if state.selected().is_none() {
            return ()
        }

        let selected_idx = state.selected().unwrap();
        let wind_height = get_window_height() as usize;
        if selected_idx > state.offset() + wind_height - 1 {
            *state.offset_mut() = selected_idx - wind_height + 1;
        } else if selected_idx < state.offset() {
            *state.offset_mut() = selected_idx;
        }
    }
}

impl StatefulWidget for List<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner_area = self.block.inner(area);
        let mut item_area = inner_area;
        item_area.height = 1;

        if self.items.is_empty() {
            Text::raw("Empty").red().render(inner_area, buf);
            self.block.render(area, buf);
            return ()
        }

        // Adjust offset firstly
        Self::adjust_offset(state);

        let mut current_number = 0;
        let mut is_selected = false;
        let mut current_idx = state.offset();

        for mut item in self.items
            .into_iter()
            .skip(state.offset())
            .take(get_window_height() as usize)
        {
            if let Some(selected_idx) = state.selected() {
                if current_idx == selected_idx {
                    item.style = item.style.reversed();
                    is_selected = true;
                }
            }


            let mut right = Line::default();

            // Set Index for each item
            if self.navi_show {
                if let Some(index) = self.navi_index {
                    let splitted = prefix_split(index, current_number);

                    if let Some(_prefix) = splitted.0 {
                        right.push_span(
                            Span::raw(_prefix)
                                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                        );
                    }

                    if let Some(_normal) = splitted.1 {
                        right.push_span(
                            Span::raw(_normal)
                                .add_modifier(Modifier::DIM)
                                .remove_modifier(Modifier::BOLD)
                        );
                    }
                } else {
                    right.push_span(
                        Span::raw(current_number.to_string())
                            .add_modifier(Modifier::DIM)
                            .remove_modifier(Modifier::BOLD)
                    );
                }

                if !is_selected {
                    right.style.fg = Some(self.index_color);
                }
            }

            // Set whitespace to make 
            if self.navi_show && item.marked_style.is_none() && self.marked {
                right.push_span(Span::raw(" "));
            }


            // Render item
            if !right.spans.is_empty() {
                item.right = Some(right);
            }

            item.render(item_area, buf);

            item_area.y += 1;
            current_idx += 1;
            current_number += 1;
            is_selected = false;
        }

        self.block.render(area, buf);
    }
}

#[inline]
fn prefix_split(
    prefix: usize,
    number: usize
) -> (Option<String>, Option<String>)
{
    let num_str = number.to_string();

    if prefix == 0 {
        if number == 0 {
            return (Some(num_str), None)
        }

        return (None, Some(num_str))
    }

    if number == prefix {
        return (Some(num_str), None)
    }

    // Calculate the pow of prefix
    let mut pow = 1;
    let mut temp = prefix;
    while temp >= 10 {
        temp /= 10;
        pow += 1;
    }

    if number / 10usize.pow(pow) != prefix {
        return (None, Some(num_str))
    }

    let prefix_side = num_str[..pow as usize].to_owned();
    let right_side = num_str[pow as usize..].to_owned();

    (Some(prefix_side), Some(right_side))
}
