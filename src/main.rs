mod ui;
mod app;
mod error;
mod config;
mod command;
mod key_event;

use std::io::{stderr, Stderr};
use std::time::Duration;

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
};

use ratatui::crossterm::{
    event::{self, KeyCode, KeyEventKind},
    terminal::{
        enable_raw_mode,
        disable_raw_mode,
        EnterAlternateScreen,
        LeaveAlternateScreen
    },
    execute
};

use error::AppResult;
use app::{App, FileContent};
use key_event::{
    handle_event,
    shell_process,
    fetch_working_directory,
    ShellCommand,
};

fn main() -> AppResult<()> {
    let mut app = App::default();
    let image_recvs = app.init_image_picker();
    let search_recv = app.init_search_channel();
    app.init_all_files()?;

    // Init config information.
    config::init_config(&mut app)?;

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    // Check, whether to enable working directory mode.
    shell_in_workdir(&mut app, &mut terminal)?;

    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen)?;

    loop {
        terminal.draw(|frame| {
            if let Err(err) = ui::ui(frame, &mut app) {
                app.app_error.add_error(err);
            }
        })?;

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

        // Search handler
        if let Ok(idx_set) = search_recv.try_recv() {
            app.file_searcher.update_idx(idx_set);

            if let Err(err) = app.next_candidate() {
                app.app_error.append_errors(err.iter());
            }
        }

        // Image perview handler
        if let Some((ref prx, ref irx)) = image_recvs {
            if let Ok(data) = irx.try_recv() {
                if let Some(image) = data {
                    match app.image_preview.make_protocol(image) {
                        Err(err) => app.app_error.add_error(err),
                        Ok(_) => app.file_content = FileContent::Image,
                    }
                } else {
                    app.file_content = FileContent::Text(String::from("Non Text File"));
                }
            }

            if let Ok(protocol) = prx.try_recv() {
                if let Some(_ref) = app.image_preview.image_protocol() {
                    _ref.set_protocol(protocol);
                }
            }
        }
    }
    
    execute!(stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    Ok(())
}

/// Check whether to enter shell directly.
fn shell_in_workdir(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<Stderr>>
) -> AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 {
        match args[1].as_ref() {
            "--working-directory" => {
                app.goto_dir(
                    fetch_working_directory().expect("Cannot fetch working directory!"),
                    None
                )?;

                shell_process(
                    app,
                    terminal,
                    ShellCommand::Shell,
                    true
                )?;
            },
            _ => ()
        }
    }

    Ok(())
}
