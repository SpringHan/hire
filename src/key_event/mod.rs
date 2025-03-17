// Key Event

mod tab;
mod shell;
mod switch;
mod interaction;
mod file_search;
mod command_line;
mod goto_operation;
mod cursor_movement;
mod file_operations;
mod paste_operation;
mod simple_operations;

use std::io::Stderr;
use std::ops::{SubAssign, AddAssign};

use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use simple_operations::output_path;
use tab::tab_operation;
use interaction::fzf_jump;
use goto_operation::goto_operation;
use paste_operation::paste_operation;
use cursor_movement::directory_movement;
use file_operations::{append_file_name, delete_operation, mark_operation};

use crate::utils::Direction;
use crate::error::AppResult;
use crate::command::AppCommand;
use crate::app::{self, App, CursorPos, OptionFor, FileOperation};

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

// Export
pub use tab::TabList;
pub use file_search::FileSearcher;
pub use cursor_movement::move_cursor;
pub use switch::{SwitchCase, SwitchCaseData};
pub use shell::{ShellCommand, CommandStr, shell_process, fetch_working_directory};

// Export for auto config
pub use tab::read_config as tab_read_config;
pub use goto_operation::read_config as goto_read_config;

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
pub fn handle_event(
    key: KeyEvent,
    app: &mut App,
    terminal: &mut Terminal
) -> AppResult<()>
{
    match key.code {
        KeyCode::Char(c) => {
            // NOTE: Maybe there'll add a function to handle character with modifiers.
            if !(key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) {
                return Ok(())
            }

            if app.command_error {
                app.quit_command_mode();
                return Ok(())
            }

            // NOTE(for coding): All the function in the blocks below must be end with
            // code to set OPTION_KEY to None.
            match app.option_key {
                OptionFor::Switch(ref case) => {
                    let case = case.to_owned();
                    switch::switch_match(app, case, c)?;
                    return Ok(())
                },
                OptionFor::None => ()
            }

            // Execute keybinding or insert character.
            if let app::Block::Browser(in_root) = app.selected_block {
                let command = app.keymap.get(c)?;
                command.execute(app, terminal, in_root)?;
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
                match cursor {
                    app::CursorPos::Index(idx) => {
                        if *idx == 0 {
                            return Ok(())
                        }
                        origin.remove(*idx - 1);
                        idx.sub_assign(1);
                    },
                    app::CursorPos::End => {
                        origin.pop();
                    },
                    _ => ()
                }
            }
        },

        KeyCode::Esc => {
            match app.selected_block {
                app::Block::CommandLine(_, _) => {
                    app.quit_command_mode();
                },
                _ => {
                    if let OptionFor::None = app.option_key {
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
                return Ok(())
            } else {
                if let app::Block::CommandLine(_, _) = app.selected_block {
                    app.command_parse(terminal)?;
                    return Ok(())
                }
            }

            if app.output_file.is_some() && app.confirm_output {
                output_path(app, false)?;
            }
        },

        // TODO: Modify these keymaps
        KeyCode::Up => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Direction::Up);
                } else {
                    app.command_select(Goto::Up);
                }
            }
        },

        KeyCode::Down => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Direction::Down);
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
                match cursor {
                    CursorPos::Index(idx) => {
                        if *idx == 0 {
                            return Ok(())
                        }
                        idx.sub_assign(1);
                    },
                    CursorPos::End => {
                        *cursor = CursorPos::Index(command.len() - 1);
                    },
                    _ => ()
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
            match key.modifiers {
                KeyModifiers::ALT => {
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

                KeyModifiers::NONE => {
                    
                },

                _ => ()
            }
        },

        _ => ()
    }

    Ok(())
}

impl AppCommand {
    pub fn execute(
        self,
        app: &mut App,
        terminal: &mut Terminal,
        in_root: bool
    ) -> AppResult<()>
    {
        match self {
            AppCommand::Tab                => tab_operation(app)?,
            AppCommand::Goto               => goto_operation(app),
            AppCommand::Paste              => paste_operation(app)?,
            AppCommand::Delete             => delete_operation(app),
            AppCommand::HideOrShow         => app.hide_or_show(None)?,
            AppCommand::FzfJump            => fzf_jump(app, terminal)?,
            AppCommand::CmdShell           => shell::cmdline_shell(app)?,
            AppCommand::PrintFullPath      => simple_operations::print_full_path(app),
            AppCommand::ChangeOutputStatus => app.confirm_output = !app.confirm_output,
            AppCommand::Search             => app.set_command_line("/", CursorPos::End),
            AppCommand::SingleSymlink      => paste_operation::make_single_symlink(app)?,

            AppCommand::AppendFsName(to_edge) => append_file_name(app, to_edge)?,
            AppCommand::Mark(single)          => mark_operation(app, single, in_root)?,

            AppCommand::CreateDir => app.set_command_line(
                ":create_dir ",
                CursorPos::End
            ),

            AppCommand::CreateFile => app.set_command_line(
                ":create_file ",
                CursorPos::End
            ),

            AppCommand::Refresh => app.goto_dir(
                app.current_path(),
                Some(app.hide_files)
            )?,

            AppCommand::Shell => shell_process(
                app,
                terminal,
                ShellCommand::Shell,
                true
            )?,

            AppCommand::ItemMove(direction) => directory_movement(
                direction,
                app,
                terminal,
                in_root
            )?,

            AppCommand::MoveCandidate(next) => if next {
                app.next_candidate()?
            } else {
                app.prev_candidate()?
            },

            AppCommand::WorkDirectory(set) => if set {
                shell::set_working_directory(
                    app.path.to_owned()
                )?
            } else {
                app.goto_dir(fetch_working_directory()?, None)?
            },

            AppCommand::GotoBottom => {
                let last_idx = if in_root {
                    app.parent_files.len() - 1
                } else {
                    app.current_files.len() - 1
                };
                move_cursor(app, Goto::Index(last_idx), in_root)?;
            },

            AppCommand::ShellCommand(cmd_vec, refresh) => {
                let cmd = cmd_vec.iter()
                    .map(|_line| {
                        if _line == "$." {
                            CommandStr::SelectedItem
                        } else {
                            CommandStr::Str(_line)
                        }
                    })
                    .collect::<Vec<_>>();

                shell_process(
                    app,
                    terminal,
                    ShellCommand::Command(None, cmd),
                    refresh
                )?
            },
        }

        Ok(())
    }
}
