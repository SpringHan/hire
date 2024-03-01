// Key Event

mod cursor_movement;
mod file_operations;
mod goto_operation;
mod shell_command;

// Export
pub use cursor_movement::move_cursor;
pub use shell_command::{ShellCommand, shell_process, fetch_working_directory};
pub use goto_operation::{init_config, GotoOperation};

use crate::App;
use crate::app::{self, CursorPos, OptionFor, FileOperation};

use std::io::Stderr;
use std::error::Error;
use std::ops::{SubAssign, AddAssign};

use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;
use crossterm::event::KeyCode;

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

/// The enum that used to declare method to move.
#[derive(PartialEq, Eq)]
pub enum Goto {
    Up,
    Down,
    Index(usize)
}

// NOTE(for coding): When quiting command-line mode, you're required to use quit_command_mode function!
// NOTE(for coding): DO NOT use return in the match control to skip specific code, which
// could cause missing the following procedures.
/// Handle KEY event.
pub fn handle_event(key: KeyCode,
                    app: &mut App,
                    terminal: &mut Terminal
) -> Result<(), Box<dyn Error>>
{
    use cursor_movement::*;
    use file_operations::*;
    use goto_operation::*;

    match key {
        KeyCode::Char(c) => {
            if let app::Block::Browser(in_root) = app.selected_block {
                // NOTE(for coding): All the function in the blocks below must be end with
                // code to set OPTION_KEY to None.
                match app.option_key {
                    OptionFor::Goto(modifying) => {
                        goto_operation(app, c, modifying, in_root)?;
                        return Ok(())
                    },
                    OptionFor::Delete => {
                        delete_operation(app, c, in_root)?;
                        return Ok(())
                    },
                    OptionFor::Paste => {
                        paste_operation(app, c)?;
                        return Ok(())
                    },
                    OptionFor::None => ()
                }

                match c {
                    'n' | 'i' | 'u' | 'e' => directory_movement(
                        c, app, terminal, in_root
                    )?,
                    'g' => app.option_key = OptionFor::Goto(GotoOperation::None),
                    'G' => {
                        let last_idx = if in_root {
                            app.parent_files.len() - 1
                        } else {
                            app.current_files.len() - 1
                        };
                        move_cursor(app, Goto::Index(last_idx), in_root)?;
                    },
                    'd' => app.option_key = OptionFor::Delete,
                    '/' => app.set_command_line("/", CursorPos::End),
                    'k' => app.next_candidate()?,
                    'K' => app.prev_candidate()?,
                    'a' => append_file_name(app, false),
                    'A' => append_file_name(app, true),
                    ' ' => mark_operation(app, true, in_root)?,
                    'm' => mark_operation(app, false, in_root)?,
                    '+' => app.set_command_line(
                        ":create_dir ",
                        CursorPos::End
                    ),
                    '=' => app.set_command_line(
                        ":create_file ",
                        CursorPos::End
                    ),
                    '-' => app.hide_or_show(None)?,
                    'p' => app.option_key = OptionFor::Paste,
                    's' => make_single_symlink(app)?,
                    'S' => shell_process(
                        app,
                        terminal,
                        ShellCommand::Shell,
                        true
                    )?,
                    'l' => shell_process(
                        app,
                        terminal,
                        ShellCommand::Command("lazygit", None),
                        true
                    )?,
                    'w' => app.goto_dir(fetch_working_directory()?)?,
                    _ => ()
                }
            } else {
                app.command_line_append(c);
            }
        },

        KeyCode::Backspace => {
            if let
                app::Block::CommandLine(
                    ref mut origin,
                    ref mut cursor
                ) = app.selected_block
            {
                if let app::CursorPos::Index(idx) = cursor {
                    if *idx == 0 {
                        return Ok(())
                    }
                    origin.remove(*idx - 1);
                    idx.sub_assign(1);
                } else {
                    origin.pop();
                }
            }
        },

        KeyCode::Esc => {
            match app.selected_block {
                app::Block::CommandLine(_, _) => {
                    app.quit_command_mode();
                },
                _ => {
                    if app.option_key != OptionFor::None {
                        app.option_key = OptionFor::None;
                        return Ok(())
                    }

                    if !app.marked_files.is_empty() {
                        app.marked_files.clear();
                        app.marked_operation = FileOperation::None;
                    }
                }
            }
        },

        KeyCode::Enter => {
            if app.command_error {
                app.quit_command_mode();
            } else {
                app.command_parse()?;
            }
        },

        KeyCode::Up => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Goto::Up);
                } else {
                    app.command_select(Goto::Up);
                }
            }
        },

        KeyCode::Down => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Goto::Down);
                } else {
                    app.command_select(Goto::Down);
                }
            }
        },

        KeyCode::Left => {
            if let
                app::Block::CommandLine(
                    ref command,
                    ref mut cursor
                ) = app.selected_block
            {
                if let CursorPos::Index(idx) = cursor {
                    if *idx == 0 {
                        return Ok(())
                    }
                    idx.sub_assign(1);
                } else {
                    *cursor = CursorPos::Index(command.len() - 1);
                }
            }
        },

        KeyCode::Right => {
            if let
                app::Block::CommandLine(
                    ref command,
                    ref mut cursor
                ) = app.selected_block
            {
                if let CursorPos::Index(idx) = cursor {
                    if *idx == command.len() - 1 {
                        *cursor = CursorPos::End;
                        return Ok(())
                    }
                    idx.add_assign(1);
                }
            }
        },

        KeyCode::Tab => {
            // TODO(to be removed): Pay attention to command_error.
            if let app::Block::CommandLine(_, _) = app.selected_block {
                // NOTE(for refactoring): Code about the close of expand mode have appeared twice.
                if app.command_expand {
                    app.command_expand = false;
                    app.command_scroll = None;
                } else {
                    app.expand_init();
                }
            }
        },

        _ => ()
    }

    Ok(())
}
