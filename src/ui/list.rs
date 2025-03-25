// List Widget

use ratatui::{buffer::Buffer, layout::Rect, text::Text, widgets::{ListState, StatefulWidget}};

pub struct Item<'a> {
    left: Text<'a>,
    right: Text<'a>
}

pub struct List<'a> {
    items: Vec<Item<'a>>,
}

impl<'a> Item<'a> {
    pub fn new(left: Text<'a>, right: Text<'a>) -> Self {
        Self { left, right }
    }
}

impl<'a> List<'a> {
    pub fn new(items: Vec<Item<'a>>) -> Self {
        Self { items }
    }
}

impl StatefulWidget for List<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        
    }
}
