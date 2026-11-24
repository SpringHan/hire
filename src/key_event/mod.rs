//! Dispatch of key presses into `AppCommand`s and editing operations.

mod tab;
mod edit;
mod shell;
mod switch;
mod macro_page;
mod interaction;
mod file_search;
mod path_history;
mod command_line;
mod sudo_session;
mod goto_operation;
mod cursor_movement;
mod file_operations;
mod paste_operation;
mod simple_operations;

use command_line::completion;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use interaction::*;
use tab::tab_operation;
use goto_operation::goto_operation;
use paste_operation::paste_operation;
use cursor_movement::{directory_movement, jump_to_index};
use path_history::{goto_prev_path, path_history_operation};
use file_operations::{append_file_name, delete_operation, mark_operation};

use crate::key_event::simple_operations::jump_to_temp_file;
use crate::rt_error;
use crate::app::App;
use crate::error::AppResult;
use crate::command::AppCommand;
use crate::utils::{
    cursor_from_insertion_point,
    cursor_insertion_point,
    delete_word,
    prev_char_boundary,
    Block,
    CmdContent,
    CursorPos,
    Direction,
};

pub use tab::{Tab, TabId, TabList};
pub use file_search::FileSearcher;
pub use sudo_session::SessionTabs;
pub use macro_page::execute_macro;
pub use edit::{EditMode, EditItem};
pub use simple_operations::output_path;
pub use switch::{
    MenuList, MenuPresentation, MenuRow, SwitchCase, SwitchCaseData
};
pub use command_line::{AppCompletion, get_content};
pub use cursor_movement::{move_cursor, Goto, NaviIndex};
pub use shell::{
    ShellCommand,
    CommandStr,
    KeySpec,
    ScrollCommand,
    ShellPrefix,
    ShellSessions,
    handle_view_mouse,
    handle_view_paste,
    open_shell,
    shell_process,
    fetch_working_directory,
    ShellOrigin,
};

// Only the tests outside `key_event` (the shell status line in `ui`) need the
// graphics support to mount a session without a terminal.
#[cfg(test)]
pub use shell::GraphicsSupport;

pub use tab::read_config as tab_read_config;
pub use goto_operation::read_config as goto_read_config;

// NOTE(for coding): leaving command-line mode must go through
// `quit_command_mode`, which does the cleanup.
pub fn handle_event(
    key: KeyEvent,
    app: &mut App,
    terminal: &mut DefaultTerminal
) -> AppResult<()>
{
    // The attached shell owns every key except its prefix chord, so it must
    // be checked before the match below.
    if app.selected_block.is_shell() {
        return shell::handle_view_key(key, app)
    }

    match key.code {
        KeyCode::Char(c) => {
            if !(key.modifiers.is_empty() ||
                 key.modifiers == KeyModifiers::SHIFT)
            {
                match key.modifiers {
                    KeyModifiers::CONTROL => {
                        match c {
                            'g' => app.command_completion.hide(),

                            // `Ctrl-b`/`Ctrl-f` scroll the expanded view
                            // sideways, but move the cursor elsewhere;
                            // `Ctrl-a`/`Ctrl-e` always move it to an edge.
                            'b' => {
                                if app.command_expand {
                                    app.expand_scroll(Direction::Left);
                                } else if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_move(false, false)?;
                                } else {
                                    app.cursor_left(false);
                                }
                            },
                            'f' => {
                                if app.command_expand {
                                    app.expand_scroll(Direction::Right);
                                } else if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_move(true, false)?;
                                } else {
                                    app.cursor_right(false);
                                }
                            },
                            'a' | 'e' => {
                                let to_end = c == 'e';

                                if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_move(to_end, true)?;
                                } else {
                                    if to_end {
                                        app.cursor_right(true);
                                    } else {
                                        app.cursor_left(true);
                                    }
                                }
                            },

                            'n' | 'p' => {
                                let next = c == 'n';

                                if app.command_expand {
                                    app.expand_scroll(
                                        if next {
                                            Direction::Down
                                        } else {
                                            Direction::Up
                                        }
                                    );
                                } else if app.command_completion.show_frame() {
                                    command_line::switch_to(app, next)?;
                                } else {
                                    app.command_select(next);
                                }
                            },

                            // Without the kitty keyboard protocol a terminal
                            // reports `Ctrl-Backspace` as `Ctrl-h`, so both it
                            // and `Ctrl-w` delete the word before the cursor.
                            'h' | 'w' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.delete_word_before_cursor();
                                } else {
                                    app.delete_word_before_cursor();
                                }
                            },

                            'd' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.delete_char_after_cursor();
                                } else {
                                    app.delete_char_after_cursor();
                                }
                            },

                            // `Ctrl-k` is the readline "kill line".
                            'k' => app.delete_to_end(),

                            _ => ()
                        }
                    },

                    KeyModifiers::ALT => {
                        match c {
                            'f' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_word_forward();
                                } else {
                                    app.cursor_word_forward();
                                }
                            },

                            'b' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.cursor_word_backward();
                                } else {
                                    app.cursor_word_backward();
                                }
                            },

                            'd' => {
                                if app.edit_mode.inserting() {
                                    app.edit_mode.delete_word_after_cursor();
                                } else {
                                    app.delete_word_after_cursor();
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

            if let Some(ref case) = app.switch_case {
                let case = case.to_owned();
                switch::switch_match(app, case, c)?;
                return Ok(())
            }

            if let Block::Browser(in_root) = app.selected_block {
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
            if key.modifiers == KeyModifiers::CONTROL &&
                matches!(app.selected_block, Block::CommandLine(_, _))
            {
                app.delete_word_before_cursor();
                return Ok(())
            }

            if key.modifiers == KeyModifiers::CONTROL &&
                app.edit_mode.inserting()
            {
                app.edit_mode.delete_word_before_cursor();
                return Ok(())
            }

            if let Block::CommandLine(
                ref mut origin,
                ref mut cursor
            ) = app.selected_block
            {
                if let CursorPos::None = cursor {
                    return Ok(())
                }

                if key.modifiers == KeyModifiers::ALT {
                    if let CmdContent::String(_str) = origin {
                        delete_word(_str, cursor);
                    }
                } else {
                    let content_ref = origin.get_mut();
                    let Some(pos) = cursor_insertion_point(content_ref, *cursor)
                    else {
                        return Ok(())
                    };

                    if let Some(start) = prev_char_boundary(content_ref, pos) {
                        content_ref.replace_range(start..pos, "");
                        *cursor = cursor_from_insertion_point(
                            content_ref,
                            start
                        );
                    }
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
                app.edit_mode.backspace(key.modifiers == KeyModifiers::ALT);
            }
        },
        
        // TODO: split these special keys out; this match has grown too long.
        KeyCode::Esc => {
            match app.selected_block {
                Block::CommandLine(_, _) => {
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
                if let Block::CommandLine(_, ref _cursor) = app.selected_block {
                    if *_cursor != CursorPos::None {
                        app.command_parse(terminal)?;
                    }

                    return Ok(())
                }
            }

            if app.navi_index.show() {
                jump_to_index(app)?;
                return Ok(())
            }

            if app.edit_mode.enabled {
                // While a name is being inserted, `Enter` must not open the
                // save menu and leave edit mode.
                if app.edit_mode.inserting() {
                    return Ok(())
                }

                SwitchCase::new(
                    app,
                    edit::save_edit,
                    MenuPresentation::Inline,
                    "Edit",
                    edit::generate_msg(),
                    SwitchCaseData::None
                );
                return Ok(())
            }

            // `Enter` writes the selected file's path; `Shift-Enter` writes the
            // current directory. The latter needs a terminal which reports the
            // Shift modifier on Enter (kitty keyboard protocol / `CSI u`),
            // because a legacy Enter byte carries no modifier.
            let file_out = !key.modifiers.contains(KeyModifiers::SHIFT);

            output_path(app, terminal, file_out)?;
        },

        KeyCode::Up => {
            match app.selected_block {
                Block::CommandLine(_, _) => {
                    // NOTE: Ctrl-p/Ctrl-n page the expanded command line.
                    if !app.command_expand {
                        app.command_select(false);
                    }
                },
                Block::Browser(_) => enter_command_history(app, false),
                Block::Shell(_) => (),
            }
        },

        KeyCode::Down => {
            match app.selected_block {
                Block::CommandLine(_, _) => {
                    if !app.command_expand {
                        app.command_select(true);
                    }
                },
                Block::Browser(_) => enter_command_history(app, true),
                Block::Shell(_) => (),
            }
        },

        KeyCode::Left => app.cursor_left(false),

        KeyCode::Right => app.cursor_right(false),

        KeyCode::Tab => handle_tab(app, key.modifiers)?,

        _ => ()
    }

    Ok(())
}

/// Handle a `Tab` press: completion, or the full-page command-line editor.
fn handle_tab(app: &mut App, modifiers: KeyModifiers) -> AppResult<()> {
    // `Alt-Ctrl-Tab` arrives as `Alt-Tab`: `Tab` is the `Ctrl-I` byte, so the
    // terminal drops the Control bit.
    if modifiers.contains(KeyModifiers::ALT) {
        if let Block::CommandLine(_, _) = app.selected_block {
            // A menu owns its presentation (`MenuPresentation`), so the chord
            // only toggles the full editor of a typed command.
            if app.switch_case.is_some() {
                return Ok(())
            }

            // NOTE(for refactoring): this inlines `App::expand_quit`, which
            // closes the expand mode the same way.
            if app.command_expand {
                app.command_expand = false;
                app.command_scroll = None;
            } else {
                app.expand_init();
            }
        }
    } else if modifiers.is_empty() {
        completion(app)?;
    }

    Ok(())
}

/// Enter command line mode with a command from the command history.
///
/// `next` is false for `Up`, which enters at the newest command, and true for
/// `Down`, which enters at the oldest.
fn enter_command_history(app: &mut App, next: bool) {
    if !app.command_history.is_empty() {
        let idx = if next {
            0
        } else {
            app.command_history.len() - 1
        };

        app.selected_block.set_command_line(
            app.command_history[idx].to_owned(),
            CursorPos::End
        );
        app.command_idx = Some(idx);
    }
}

impl AppCommand {
    pub fn execute(
        self,
        app: &mut App,
        terminal: &mut DefaultTerminal,
        in_root: bool
    ) -> AppResult<()>
    {
        match self {
            AppCommand::NextTab         => {tab::next(app)?;},
            AppCommand::PrevTab         => {tab::prev(app)?;},
            AppCommand::Tab             => tab_operation(app)?,
            AppCommand::Goto            => goto_operation(app),
            AppCommand::Paste           => paste_operation(app)?,
            AppCommand::Delete          => delete_operation(app),
            AppCommand::ShowNaviIndex   => app.navi_index.init(),
            AppCommand::MarkExpand      => app.mark_expand = true,
            AppCommand::OutputFile      => output_path(app, terminal, true)?,
            AppCommand::GotoFile        => jump_to_temp_file(app)?,
            AppCommand::PrevPath        => goto_prev_path(app)?,
            AppCommand::EditDelete      => edit::mark_delete(app)?,
            AppCommand::HideOrShow      => app.hide_or_show(None)?,
            AppCommand::RgSearch        => rg_jump(app, terminal)?,
            AppCommand::FzfJump         => fzf_jump(app, terminal)?,
            AppCommand::VimDiff         => vim_diff(app, terminal)?,
            AppCommand::CmdShell        => shell::cmdline_shell(app)?,
            AppCommand::ShellMenu       => shell::shell_operation(app)?,
            AppCommand::ShellAttach     => shell::attach_active(app)?,
            AppCommand::AttachPinned    => shell::attach_pinned(app)?,
            AppCommand::PathHistory     => path_history_operation(app),
            AppCommand::MacroPage       => macro_page::macro_operation(app)?,
            AppCommand::PrintFullPath   => simple_operations::print_full_path(app),
            AppCommand::SingleSymlink   => paste_operation::make_single_symlink(app)?,
            AppCommand::SudoSession     => sudo_session::sudo_session(app, terminal)?,
            AppCommand::EditGotoTop     => edit::item_navigation(app, Goto::Index(0))?,
            AppCommand::QuitAfterOutput => app.quit_after_output = !app.quit_after_output,
            AppCommand::ReloadConfig    => crate::config::load_config(app)?,


            AppCommand::NaviIndexInput(idx)   => app.navi_index.input(idx),
            AppCommand::SwitchTab(idx)        => tab::quick_switch(app, idx)?,
            AppCommand::AppendFsName(to_edge) => append_file_name(app, to_edge)?,
            AppCommand::EditMark(single)      => edit::mark_operation(app, single)?,
            AppCommand::Mark(single)          => mark_operation(app, single, in_root)?,

            AppCommand::QuitEdit => {
                app.edit_mode.reset();
                app.mark_expand = false;
            },

            AppCommand::EnterCommandLine => app.enter_command_line(),

            AppCommand::EditInsert(end) => app.edit_mode.enter_insert(
                &mut app.selected_item.current,
                if end {
                    CursorPos::End
                } else {
                    CursorPos::Index(0)
                }
            ),

            AppCommand::EditAppendFsName => app.edit_mode
                .enter_insert_before_extension(
                    &mut app.selected_item.current
                ),

            AppCommand::Search  => app.selected_block.set_command_line(
                "/",
                CursorPos::End
            ),

            AppCommand::PreviewSearch => app.selected_block.set_command_line(
                "?",
                CursorPos::End
            ),

            AppCommand::PreviewScrollPage(down) => app.preview_scroll_page(down),

            AppCommand::PreviewScrollLine(down) => app.preview_scroll_line(down),

            AppCommand::PreviewScrollColumn(right) => {
                app.preview_scroll_column(right)
            },

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

            AppCommand::Shell => shell::open_shell(app, shell::ShellOrigin::Browser)?,

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

            // A preview search owns `n`/`N` while it is active: the file
            // list's own matches are only used when the preview has no query.
            AppCommand::MoveCandidate(next) => if app.preview_search_active() {
                app.preview_move_candidate(next);
            } else if next {
                app.next_candidate()?
            } else {
                app.prev_candidate()?
            },

            AppCommand::WorkDirectory(set) => if set {
                let path = app.path.to_owned();
                shell::set_working_directory(app, path)?
            } else {
                let working_dir = match fetch_working_directory() {
                    Ok(working_dir) => working_dir,
                    Err(err) => {
                        app.working_directory = None;
                        return Err(err)
                    },
                };
                app.working_directory = Some(working_dir.to_owned());

                if app.path != working_dir {
                    app.record_path_history();
                    app.goto_dir(working_dir, None)?;
                }
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
                let cmd = CommandStr::from_slices(
                    cmd_vec.iter()
                        .map(|_line| _line.as_str())
                        .collect()
                );

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

#[cfg(test)]
mod test {
    use super::*;

    fn app_with_command_line() -> App<'static> {
        App {
            selected_block: Block::CommandLine(
                CmdContent::String(String::from(":reload")),
                CursorPos::End
            ),
            ..Default::default()
        }
    }

    /// The editor chord is `Alt-Ctrl-Tab`, but the terminal drops the Control
    /// bit (Tab already is the `Ctrl-I` byte), so it arrives as Alt+Tab.
    #[test]
    fn test_alt_ctrl_tab_toggles_the_full_page_editor() {
        let mut app = app_with_command_line();

        handle_tab(&mut app, KeyModifiers::ALT).unwrap();
        assert!(app.command_expand);

        handle_tab(&mut app, KeyModifiers::ALT).unwrap();
        assert!(!app.command_expand);

        // A terminal which does report both modifiers still works.
        handle_tab(&mut app, KeyModifiers::ALT | KeyModifiers::CONTROL)
            .unwrap();
        assert!(app.command_expand);
    }

    /// A reported `Ctrl-Tab` is ignored (the chord is the plain `Tab` byte),
    /// and a menu keeps its own presentation.
    #[test]
    fn test_the_chord_needs_alt_and_ignores_a_menu() {
        let mut app = app_with_command_line();

        handle_tab(&mut app, KeyModifiers::CONTROL).unwrap();
        assert!(!app.command_expand);

        fn stub(_: &mut App, _: char, _: SwitchCaseData) -> AppResult<bool> {
            Ok(true)
        }

        SwitchCase::new(
            &mut app,
            stub,
            MenuPresentation::Inline,
            "Tabs",
            CmdContent::String(String::from("menu")),
            SwitchCaseData::None
        );

        handle_tab(&mut app, KeyModifiers::ALT).unwrap();
        assert!(!app.command_expand);
    }
}
