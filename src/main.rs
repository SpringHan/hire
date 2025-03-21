mod ui;
mod app;
mod utils;
mod error;
mod config;
mod command;
mod key_event;

use std::time::Duration;
use std::io::{stderr, Stderr};

use clap::Parser;
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
    ShellCommand,
    handle_event,
    shell_process,
    fetch_working_directory,
};

fn main() -> AppResult<()> {
    let args = utils::Args::parse();
    
    let mut app = App::default();
    let image_recvs = app.init_image_picker();
    let search_recv = app.init_search_channel();
    app.init_all_files()?;

    // Init config information.
    config::init_config(&mut app)?;

    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    // Check, whether to enable working directory mode.
    check_passive_mode(&args, &mut app);
    check_start_path(&args, &mut app)?;
    shell_in_workdir(&args, &mut app, &mut terminal)?;

    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen)?;

    loop {
        if app.quit_now {
            break;
        }

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
    args: &utils::Args,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<Stderr>>
) -> AppResult<()> {
    if args.working_directory {
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
    }

    Ok(())
}

/// Check whether to enter passive output mode.
fn check_passive_mode(args: &utils::Args, app: &mut App) {
    if &args.output_file != "NULL" {
        app.output_file = Some(std::path::PathBuf::from(
            &args.output_file
        ));
    }
}

fn check_start_path(args: &utils::Args, app: &mut App) -> AppResult<()> {
    if &args.start_from != "NULL" {
        let mut _path = args.start_from.to_owned();
        if _path.len() > 1 && _path.ends_with("/") {
            _path.pop();
        }

        std::fs::File::open(&_path)?;

        app.goto_dir(_path, None)?;
    }

    Ok(())
}
