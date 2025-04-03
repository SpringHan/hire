// Key Event

mod tab;
mod edit;
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
use std::ops::SubAssign;

use command_line::completion;
use ratatui::Terminal as RTerminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use tab::tab_operation;
use interaction::fzf_jump;
use simple_operations::output_path;
use goto_operation::goto_operation;
use paste_operation::paste_operation;
use cursor_movement::{directory_movement, jump_to_index};
use file_operations::{append_file_name, delete_operation, mark_operation};

use crate::rt_error;
use crate::utils::Direction;
use crate::error::AppResult;
use crate::command::AppCommand;
use crate::app::{self, App, CursorPos};

type Terminal = RTerminal<CrosstermBackend<Stderr>>;

// Export
pub use tab::TabList;
pub use file_search::FileSearcher;
pub use edit::{EditMode, EditItem};
pub use switch::{SwitchCase, SwitchCaseData};
pub use command_line::{AppCompletion, get_content};
pub use cursor_movement::{move_cursor, Goto, NaviIndex};
pub use shell::{ShellCommand, CommandStr, shell_process, fetch_working_directory};

// Export for auto config
pub use tab::read_config as tab_read_config;
pub use goto_operation::read_config as goto_read_config;

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
            // Handle keys with modifiers
            if !(key.modifiers.is_empty() ||
                 key.modifiers == KeyModifiers::SHIFT)
            {
                match key.modifiers {
                    KeyModifiers::CONTROL => {
                        match c {
                            'g' => app.command_completion.hide(),
                            'b' | 'a' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_move(false, c == 'a')?;
                                } else {
                                    app.cursor_left(c == 'a');
                                }
                            },
                            'f' | 'e' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_move(true, c == 'e')?;
                                } else {
                                    app.cursor_right(c == 'e');
                                }
                            },

                            'n' | 'p' => {
                                if app.command_completion.show_frame() {
                                    command_line::switch_to(app, c == 'n')?;
                                } else {
                                    app.command_select(c == 'n');
                                }
                            },

                            _ => ()
                        }
                    },

                    _ => ()
                }

                return Ok(())
            }

            if app.command_error {
                app.quit_command_mode();
                return Ok(())
            }

            // NOTE(for coding): All the function in the blocks below must be end with
            // code to set OPTION_KEY to None.
            if let Some(ref case) = app.switch_case {
                let case = case.to_owned();
                switch::switch_match(app, case, c)?;
                return Ok(())
            }

            // Execute keybinding or insert character.
            if let app::Block::Browser(in_root) = app.selected_block {
                if app.edit_mode.inserting() {
                    app.edit_mode.insert_char(c);
                    return Ok(())
                }

                let command = if app.navi_index.show() {
                    app.keymap.navi_get(c)?
                } else if app.edit_mode.enabled {
                    app.keymap.edit_get(c)?
                } else {
                    app.keymap.get(c)?
                };

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
                let content_ref = origin.get_mut();
                match cursor {
                    app::CursorPos::Index(idx) => {
                        if *idx == 0 {
                            return Ok(())
                        }
                        content_ref.remove(*idx - 1);
                        idx.sub_assign(1);
                    },
                    app::CursorPos::End => {
                        content_ref.pop();
                    },
                    _ => ()
                }

                if !app.command_completion.popup_info().0.is_empty() {
                    app.command_completion.reset();
                }

                return Ok(())
            }

            if app.navi_index.show() {
                app.navi_index.backspace();
            }

            if app.edit_mode.inserting() {
                app.edit_mode.backspace();
            }
        },
        
        // TODO: Split this special keys function to independent functions.
        KeyCode::Esc => {
            match app.selected_block {
                app::Block::CommandLine(_, _) => {
                    if app.command_completion.show_frame() {
                        app.command_completion.hide();
                        return Ok(())
                    }

                    app.quit_command_mode();
                },
                _ => {
                    if let Some(_) = app.switch_case {
                        app.switch_case = None;
                        return Ok(())
                    }

                    if app.navi_index.show() {
                        app.navi_index.reset();
                        return Ok(())
                    }

                    if app.mark_expand {
                        app.mark_expand = false;
                        return Ok(())
                    }

                    if app.edit_mode.enabled {
                        if app.edit_mode.inserting() {
                            app.edit_mode.escape_insert(
                                &mut app.selected_item.current,
                                app.current_files.len()
                            );
                            return Ok(())
                        }

                        if app.edit_mode.has_marked() {
                            edit::mark_operation(app, false)?;
                            return Ok(())
                        }

                        app.edit_mode.reset();
                        return Ok(())
                    }

                    if !app.marked_files.is_empty() {
                        app.marked_files.clear();
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

            if app.navi_index.show() {
                jump_to_index(app)?;
                return Ok(())
            }

            if app.edit_mode.enabled {
                // Avoid unexpected exit when editing files.
                if app.edit_mode.inserting() {
                    return Ok(())
                }

                SwitchCase::new(
                    app,
                    edit::save_edit,
                    false,
                    edit::generate_msg(),
                    SwitchCaseData::None
                );
            }

            if app.output_file.is_some() && app.confirm_output {
                output_path(app, false)?;
            }
        },

        KeyCode::Up => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Direction::Up);
                } else {
                    app.command_select(false);
                }
            }
        },

        KeyCode::Down => {
            if let app::Block::CommandLine(_, _) = app.selected_block {
                if app.command_expand {
                    app.expand_scroll(Direction::Down);
                } else {
                    app.command_select(true);
                }
            }
        },

        KeyCode::Left => {
            if app.command_expand && key.modifiers == KeyModifiers::ALT {
                app.expand_scroll(Direction::Left);
                return Ok(())
            }
            app.cursor_left(false)
        },

        KeyCode::Right => {
            if app.command_expand && key.modifiers == KeyModifiers::ALT {
                app.expand_scroll(Direction::Right);
                return Ok(())
            }
            app.cursor_right(false)
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
                    completion(app)?;
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
            AppCommand::ShowNaviIndex      => app.navi_index.init(),
            AppCommand::MarkExpand         => app.mark_expand = true,
            AppCommand::EditDelete         => edit::mark_delete(app)?,
            AppCommand::HideOrShow         => app.hide_or_show(None)?,
            AppCommand::FzfJump            => fzf_jump(app, terminal)?,
            AppCommand::CmdShell           => shell::cmdline_shell(app)?,
            AppCommand::PrintFullPath      => simple_operations::print_full_path(app),
            AppCommand::ChangeOutputStatus => app.confirm_output = !app.confirm_output,
            AppCommand::SingleSymlink      => paste_operation::make_single_symlink(app)?,
            AppCommand::EditGotoTop        => edit::item_navigation(app, Goto::Index(0))?,

            AppCommand::NaviIndexInput(idx)   => app.navi_index.input(idx),
            AppCommand::AppendFsName(to_edge) => append_file_name(app, to_edge)?,
            AppCommand::EditMark(single)      => edit::mark_operation(app, single)?,
            AppCommand::Mark(single)          => mark_operation(app, single, in_root)?,

            AppCommand::QuitEdit => {
                app.edit_mode.reset();
                app.mark_expand = false;
            },

            AppCommand::EditInsert(end) => app.edit_mode.enter_insert(
                &mut app.selected_item.current,
                if end {
                    CursorPos::End
                } else {
                    CursorPos::Index(0)
                }
            ),

            AppCommand::Search  => app.selected_block.set_command_line(
                "/",
                CursorPos::End
            ),

            AppCommand::CreateDir => app.selected_block.set_command_line(
                ":create_dir ",
                CursorPos::End
            ),

            AppCommand::CreateFile => app.selected_block.set_command_line(
                ":create_file ",
                CursorPos::End
            ),

            AppCommand::EditNew(is_idr) => app.edit_mode.create_item(
                &mut app.selected_item.current,
                is_idr
            )?,

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

            AppCommand::EditMoveItem(next) => edit::item_navigation(
                app,
                if next {
                    Goto::Down
                } else {
                    Goto::Up
                }
            )?,

            AppCommand::EditGotoBottom => edit::item_navigation(
                app,
                Goto::Index(app.edit_mode.len() - 1)
            )?,

            AppCommand::ListScroll(down) => move_cursor(
                app,
                if down {
                    Goto::ScrollDown
                } else {
                    Goto::ScrollUp
                },
                app.root()
            )?,

            AppCommand::EditListScroll(down) => edit::item_navigation(
                app,
                if down {
                    Goto::ScrollDown
                } else {
                    Goto::ScrollUp
                }
            )?,

            AppCommand::EditMode => if !app.root() {
                app.edit_mode.init(app.current_files.iter());
            } else {
                rt_error!("Edit mode cannot enabled in the root directory")
            },

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
