// Macro page.

use ratatui::text::{Line, Text};
use ratatui::DefaultTerminal;

use super::{handle_event, SwitchCase, SwitchCaseData};

use crate::app::{App, MacroStatus, MacroTarget};
use crate::error::{AppError, AppResult};
use crate::utils::{Block, CmdContent};
use crate::rt_error;

/// Open the macro page.
///
/// In the page, `q` is used to record a macro and `e` is used to execute it.
pub fn macro_operation(app: &mut App) -> AppResult<()> {
    if app.macro_attri.status == MacroStatus::Recording {
        app.macro_attri.status = MacroStatus::None;
        rt_error!("You should press 'q' to stop recording! And now it's stopped.")
    }

    SwitchCase::new(
        app,
        macro_switch,
        true,
        generate_msg(app),
        SwitchCaseData::None
    );

    Ok(())
}

fn macro_switch(
    app: &mut App,
    key: char,
    _: SwitchCaseData
) -> AppResult<bool>
{
    match key {
        // `q` starts recording a macro,
        'q' => {
            app.macro_attri.clear_keys();
            app.macro_attri.status = MacroStatus::Recording;
        },

        // Execute the recorded macro.
        'e' => {
            if app.macro_attri.status == MacroStatus::Executing {
                return Ok(true)
            }

            if app.macro_attri.is_empty() {
                rt_error!("No macro has been recorded!")
            }

            // When there're marked files, the macro will be executed on every
            // marked file. So the targets are recorded here and the macro is
            // really executed by `execute_macro` later.
            let targets = if app.marked_files.is_empty() {
                Vec::new()
            } else {
                collect_marked_targets(app)
            };

            app.macro_attri.set_targets(targets);
            app.macro_attri.status = MacroStatus::Executing;
        },

        _ => ()
    }

    Ok(true)
}

/// Execute the recorded macro.
///
/// When the macro has targets (which are the marked files), it will be
/// executed on every target after selecting it. Otherwise, it will be
/// executed on the currently selected item.
pub fn execute_macro(
    app: &mut App,
    terminal: &mut DefaultTerminal
) -> AppResult<()>
{
    let keys = app.macro_attri.get_keys();
    let targets = app.macro_attri.take_targets();

    if targets.is_empty() {
        for key in keys.iter() {
            handle_event(*key, app, terminal)?;
        }

        return Ok(())
    }

    let mut errors = AppError::new();

    for target in targets {
        match select_target(app, &target) {
            Ok(true) => (),
            // The target does not exist any more, skip it.
            Ok(false) => continue,
            Err(err) => {
                errors.append_errors(err.iter());
                continue;
            }
        }

        for key in keys.iter() {
            if let Err(err) = handle_event(*key, app, terminal) {
                errors.append_errors(err.iter());
                break
            }
        }
    }

    if errors.is_empty() {
        return Ok(())
    }

    Err(errors)
}

/// Collect the marked files as the targets of a macro.
fn collect_marked_targets(app: &App) -> Vec<MacroTarget> {
    let mut targets = app.marked_files
        .iter()
        .flat_map(|(dir, marked_files)| {
            marked_files.files.keys().map(move |name| MacroTarget {
                dir: dir.to_owned(),
                name: name.to_owned()
            })
        })
        .collect::<Vec<_>>();

    // Make the order of execution stable.
    targets.sort_by(|a, b| {
        a.dir.cmp(&b.dir).then_with(|| a.name.cmp(&b.name))
    });

    targets
}

/// Select the target item in the file browser.
///
/// Return true if the target item has been selected successfully.
fn select_target(app: &mut App, target: &MacroTarget) -> AppResult<bool> {
    if let Block::CommandLine(_, _) = app.selected_block {
        app.quit_command_mode();
    }

    if app.path != target.dir {
        let hide_files = app.hide_files;
        app.goto_dir(&target.dir, Some(hide_files))?;
    }

    app.update_with_prev_selected(Some(target.name.to_owned()))?;

    Ok(
        app.get_file_saver()
            .map(|file| file.name == target.name)
            .unwrap_or(false)
    )
}

fn generate_msg(app: &App) -> CmdContent {
    let mut msg = Text::raw("[q] record macro  [e] execute macro");

    msg.push_line(Line::raw(
        if app.macro_attri.is_empty() {
            "No macro!"
        } else {
            "Recorded!"
        }
    ));

    CmdContent::Text(msg)
}
