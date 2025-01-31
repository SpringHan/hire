mod app;
mod ui;
mod key_event;

use std::io::stderr;
use std::error::Error;
use key_event::{fetch_working_directory, handle_event, shell_process, ShellCommand};
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

    // Init config information.
    key_event::init_config(&mut app)?;

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    // Check, whether to enable working directory mode.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 {
        match args[1].as_ref() {
            "--working-directory" => {
                app.goto_dir(
                    fetch_working_directory().expect("Cannot fetch working directory!"),
                    None
                )?;
                shell_process(
                    &mut app,
                    &mut terminal,
                    ShellCommand::Shell,
                    true
                )?;
            },
            _ => ()
        }
    }

    loop {
        terminal.draw(|frame| ui::ui(frame, &mut app))?;
        if event::poll(Duration::from_millis(200))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('q') {
                        if let app::Block::Browser(_) = app.selected_block {
                            break;
                        }
                    }

                    let result = handle_event(key.code, &mut app, &mut terminal);
                    if let Err(err) = result {
                        app.app_error.append_errors(err.iter());
                    }
                }
            }
        }

        if app.need_to_jump {
            let result = app.next_candidate();
            if let Err(err) = result {
                app.app_error.append_errors(err.iter());
            }
        }
    }
    
    execute!(stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
