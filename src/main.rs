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
    // println!("{}", app.selected_item.0);
    // println!("{:?}", app.current_files[0].size);

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    loop {
        terminal.draw(|frame| ui::ui(frame, &app))?;
        if event::poll(Duration::from_millis(200))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('q') {
                        break;
                    }
                    handle_event(key.code, &mut app)?;
                }
            }
        }
    }
    
    execute!(stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
