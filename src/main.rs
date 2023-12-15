mod app;
mod ui;
mod key_event;

use std::io::stderr;
use std::error::Error;
use key_event::handle_event;
use ratatui::{
    backend::CrosstermBackend,
    Terminal
};
use crossterm::{
    event::{self, KeyCode, KeyEventKind},
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute
};
use std::time::Duration;

use app::App;

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen)?;

    let mut app = App::default();
    app.init_all_files()?;
    // println!("{:#?} {:#?} {:#?}", app.parent_files, app.current_files, app.child_files);
    // println!("{:#?}", app.current_files);
    // println!("{}", app.selected_item.0);
    // println!("{:?}", app.current_files[0].size);

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    loop {
        // NOTE: Debug
        // use ratatui::text::{Span, Line};
        // use ratatui::style::{Stylize, Modifier};
        // use ratatui::widgets::Paragraph;
        // terminal.draw(|frame| {
        //     let a = vec![
        //         Span::raw("Test").add_modifier(Modifier::BOLD).add_modifier(Modifier::ITALIC),
        //         Span::raw("Test").add_modifier(Modifier::BOLD),
        //         Span::raw("Test").add_modifier(Modifier::ITALIC)
        //     ];
        //     let b = Line::from(a);
        //     frame.render_widget(Paragraph::new(b), frame.size());
        // })?;

        terminal.draw(|frame| ui::ui(frame, &mut app))?;
        if event::poll(Duration::from_millis(200))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            if let app::Block::Browser(_) = app.selected_block {
                                break;
                            }
                        },
                        other => handle_event(
                            other,
                            &mut app,
                            &mut terminal
                        )?
                    }
                }
            }
        }
    }
    
    execute!(stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
