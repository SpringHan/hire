// List Widget

use ratatui::{
    widgets::{Block, ListState, StatefulWidget, Widget},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Styled, Stylize},
    buffer::Buffer,
    text::Text,
};

use crate::utils::get_window_height;

pub struct Item<'a> {
    style: Style,
    left: Text<'a>,
    right: Option<Text<'a>>,

    empty_list: bool,
}

pub struct List<'a> {
    block: Block<'a>,
    items: Vec<Item<'a>>,

    // Navigation index
    navi_index: bool,
    index_color: Color,
}

impl<'a> Item<'a> {
    pub fn new<T>(left: T, right: Option<T>) -> Self
    where T: Into<Text<'a>>
    {
        let _right = if let Some(text) = right {
            Some(text.into())
        } else {
            None
        };

        Self {
            right: _right,
            left: left.into(),
            empty_list: false,
            style: Style::default(),
        }
    }

    pub fn empty() -> Self {
        Self {
            right: None,
            left: Text::raw("Empty").red(),

            empty_list: true,
            style: Style::default(),
        }
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

        if self.right.is_none() {
            self.left.render(area, buf);
            return ()
        }

        let right = self.right.unwrap();

        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
                Constraint::Min(right.width() as u16)
            ])
            .split(area);

        self.left.render(layout[0], buf);
        right.render(layout[1], buf);
    }
}

impl<'a> List<'a> {
    pub fn new(block: Block<'a>, items: Vec<Item<'a>>) -> Self {
        Self {
            block,
            items,
            navi_index: false,
            index_color: Color::default()
        }
    }

    pub fn index(mut self, enable: bool, style: Style) -> Self {
        self.navi_index = enable;

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
        // Adjust offset firstly
        Self::adjust_offset(state);

        let inner_area = self.block.inner(area);
        let mut item_area = inner_area;
        item_area.height = 1;

        let mut current_number = 0;
        let mut is_selected = false;
        let mut current_idx = state.offset();

        for mut item in self.items
            .into_iter()
            .skip(state.offset())
            .take(get_window_height() as usize)
        {
            if item.empty_list {
                item.render(item_area, buf);
                self.block.render(area, buf);

                return ()
            }

            if let Some(selected_idx) = state.selected() {
                if current_idx == selected_idx {
                    item.style = item.style.reversed();
                    is_selected = true;
                }
            }

            // Set Index for each item
            if self.navi_index {
                let mut right = Text::raw(current_number.to_string());

                if !is_selected {
                    right.style.fg = Some(self.index_color);
                }

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
