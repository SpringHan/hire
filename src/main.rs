mod ui;
mod app;
mod utils;
mod error;
mod config;
mod command;
mod key_event;

use std::time::Duration;

use clap::Parser;
use ratatui::text::Text;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, KeyCode, KeyEventKind};

use app::App;
use error::AppResult;
use utils::FileContent;
use key_event::{
    ShellCommand,
    handle_event,
    shell_process,
    fetch_working_directory,
};

fn main() -> AppResult<()> {
    let args = utils::Args::parse();

    let mut initial = true;
    let mut app = App::default();
    let image_recvs = app.init_image_picker();
    let search_recv = app.init_search_channel();

    // Init config information.
    config::init_config(&mut app)?;

    let mut terminal = ratatui::init();

    // Check, whether to enable working directory mode.
    check_output(&args, &mut app);
    check_start_path(&args, &mut app)?;
    shell_in_workdir(&args, &mut app, &mut terminal)?;

    loop {
        if app.quit_now {
            break;
        }

        terminal.draw(|frame| {
            if initial {
                initial = false;

                if let Err(err) = app.init_all_files() {
                    app.app_error.append_errors(err.iter());
                }
                ui::update_file_linenr(frame.area());
            }

            if let Err(err) = ui::ui(frame, &mut app) {
                app.app_error.add_error(err);
            }
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('q') &&
                        key.modifiers.is_empty()
                    {
                        match check_quit_condition(&mut app) {
                            QuitCheckRes::Quit => break,
                            QuitCheckRes::Reset => continue,
                            QuitCheckRes::Continue => (),
                        }
                    }

                    let result = handle_event(key, &mut app, &mut terminal);
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
            if app.image_preview.useless {
                continue;
            }

            if let Ok(data) = irx.try_recv() {
                if let Some(image) = data {
                    match app.image_preview.make_protocol(image) {
                        Err(err) => app.app_error.add_error(err),
                        Ok(_) => app.file_content = FileContent::Image,
                    }
                } else {
                    app.file_content = FileContent::Text(
                        Text::raw("Non Text File")
                    );
                }
            }

            if let Ok(response) = prx.try_recv() {
                if let Some(_ref) = app.image_preview.image_protocol() {
                    _ref.update_resized_protocol(
                        response.expect("Failed to get image resize response!")
                    );
                }
            }
        }
    }
    
    ratatui::restore();
    Ok(())
}

/// Check whether to enter shell directly.
fn shell_in_workdir(
    args: &utils::Args,
    app: &mut App,
    terminal: &mut DefaultTerminal
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
fn check_output(args: &utils::Args, app: &mut App) {
    if &args.output_file != "NULL" {
        app.output_file = args.output_file.to_owned();
    }

    app.quit_after_output = args.quit_after_output;
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


// Check before quit

enum QuitCheckRes {
    Quit,

    /// Stop running following code & start a new loop.
    Reset,

    /// Continue to run `handle_event` for 'q'.
    Continue
}

/// Conditions to check before quiting.
fn check_quit_condition(app: &mut App) -> QuitCheckRes {
    if let utils::Block::CommandLine(_, _) = app.selected_block {
        return QuitCheckRes::Continue
    }

    use ratatui::style::Stylize;
    use crate::key_event::{SwitchCase, SwitchCaseData};

    if app.tab_list.len() > 1 {
        SwitchCase::new(
            app,
            really_quit,
            false,
            crate::utils::CmdContent::Text(
                Text::raw(
                    "There're other tabs opened, are you sure to quit? (y for yes)",
                ).red()
            ),
            SwitchCaseData::None
        );
        return QuitCheckRes::Reset
    }

    QuitCheckRes::Quit
}

/// Really quit hire?
fn really_quit(
    app: &mut App,
    key: char,
    _: crate::key_event::SwitchCaseData
) -> AppResult<bool>
{
    if key == 'y' {
        app.quit_now = true;
    }

    Ok(true)
}
