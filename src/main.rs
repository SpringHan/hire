mod ui;
mod app;
mod error;
mod key_event;

use std::io::stderr;
use std::thread;
use std::time::Duration;

use ratatui::{
    backend::CrosstermBackend,
    Terminal
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

use app::App;
use error::AppResult;
use key_event::{
    fetch_working_directory,
    handle_event,
    shell_process,
    ShellCommand
};

fn main() -> AppResult<()> {
    let mut app = App::default();
    let resize_rx = app.init_image_picker();
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

    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen)?;

    // Spawn thread for image resize
    // let thread_protocol = app.image_preview.image_protocol();
    // if resize_rx.is_some() {
    //     thread::spawn(move || {
    //         let rx = resize_rx.unwrap();

    //         loop {
    //             if let Ok(protocol) = rx.recv() {
    //                 if let Ok(mut _mutex) = thread_protocol.lock() {
    //                     if let Some(ref mut thread_protocol) = *_mutex {
    //                         thread_protocol.set_protocol(protocol);
    //                     }
    //                 }
    //                 // if let Some(thread_protocol) = thread_protocol {
    //                 //     thread_protocol.set_protocol(protocol);
    //                 // }
    //             }
    //         }
    //     });
    // }

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

        if app.need_to_jump {
            let result = app.next_candidate();
            if let Err(err) = result {
                app.app_error.append_errors(err.iter());
            }
        }

        if let Some(ref rx) = resize_rx {
            if let Ok(protocol) = rx.try_recv() {
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
