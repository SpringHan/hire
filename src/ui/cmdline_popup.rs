// Popup window for command line.

use std::borrow::Cow;

use ratatui::{
    widgets::{Block, Borders, Clear, List, ListItem, ListState, StatefulWidget, Widget},
    style::{Style, Stylize},
    layout::Rect,
    Frame
};

use crate::{app::App, key_event::get_content};

pub struct CompletionPopup<'a> {
    candidates: &'a Vec<Cow<'a, str>>
}

impl<'a> CompletionPopup<'a> {
    pub fn new(candidates: &'a Vec<Cow<'a, str>>) -> Self {
        Self { candidates }
    }
}

impl StatefulWidget for CompletionPopup<'_> {
    type State = ListState;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State
    )
    {
        Clear.render(area, buf);

        let block = Block::default()
            .borders(Borders::ALL);

        let completion_list = List::new(
            self.candidates.iter()
                .map(|_candidate| ListItem::new(_candidate.as_ref()))
                .collect::<Vec<_>>()
        )
            .highlight_style(Style::default().white().reversed())
            .block(block);

        StatefulWidget::render(
            completion_list,
            area,
            buf,
            state
        );
    }
}

pub fn render_completion(
    app: &mut App,
    frame: &mut Frame,
    mut area: Rect,
)
{
    if !app.command_completion.show_frame() {
        return ()
    }

    let content_info = get_content(&app.selected_block);
    if content_info.is_none() {
        return ()
    }

    let (origin_len, max_length) = app.command_completion.popup_position();
    let (candidates, list_state) = app.command_completion
        .popup_info();

    if candidates.is_empty() {
        return ()
    }

    // Adjust area for completion popup window
    if app.command_expand {
        area.y += 1;
        area.x += origin_len - 1;
    } else {
        area.x += origin_len - 1;
        area_minus(&mut area.y, 5);
    }

    area.height = 5;
    area.width = max_length + 2;
    
    if area.x + area.width > frame.area().width {
        area.x = frame.area().width - area.width;
    }
    
    frame.render_stateful_widget(
        CompletionPopup::new(candidates),
        area,
        list_state
    );
}

fn area_minus(_num: &mut u16, rhs: u16) {
    if rhs > *_num {
        return ()
    }

    *_num -= rhs;
}
