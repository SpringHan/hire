mod app;
mod ui;

use std::io::stderr;
use std::error::Error;
use ratatui::{
    backend::{CrosstermBackend, Backend, self},
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
    // println!("{}", app.selected_item.0);

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    loop {
        terminal.draw(|frame| ui::ui(frame, &app))?;
        if event::poll(Duration::from_millis(200))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    execute!(stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
