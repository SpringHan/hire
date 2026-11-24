//! Frame assembly: the browser columns, the title bar and the overlays.

mod list;
mod utils;
mod child_block;
mod command_line;
mod parent_block;
mod current_block;
mod cmdline_popup;
mod menu_block;
mod shell_block;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    widgets::{Block, Paragraph},
    style::{Modifier, Stylize},
    text::{Line, Span},
    Frame
};

use crate::{app::App, utils::{self as cutils, CursorPos, CmdContent}};

use command_line::*;
use parent_block::render_parent;
use current_block::render_current;
use cmdline_popup::render_completion;
use child_block::{render_child, render_file};

pub use child_block::update_file_linenr;
pub use utils::key_hints;

pub fn ui(frame: &mut Frame, app: &mut App) -> anyhow::Result<()> {
    // A menu is an overlay: the page it was opened from stays visible, dimmed.
    // `Alt-Ctrl-Tab` only expands a typed command line, so an expanded block
    // without a switch case is the full-page editor, not a menu.
    let menu_open = app.command_expand && app.switch_case.is_some();
    let full_page = app.command_expand && !menu_open;

    // The bottom band belongs to the command line; only the full-page editor
    // takes it away.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if !full_page {
            vec![
                Constraint::Percentage(4),
                Constraint::Percentage(93),
                Constraint::Percentage(2)
            ]
        } else {
            vec![
                Constraint::Percentage(4),
                Constraint::Percentage(96)
            ]
        })
        .split(frame.area());

    let (title_right, right_length) = get_title_right_info(app);
    let title_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(100),
            Constraint::Min(right_length as u16)
        ])
        .split(chunks[0]);
    
    let computer_info_block = Block::default();
    let title_path = title_bar_path(app);
    let computer_info = Paragraph::new(
        Line::from(
            vec![
                Span::styled(
                    format!("{}@{}", app.user_name, app.computer_name),
                    app.term_colors.executable_style
                ),
                Span::styled(
                    format!("  {}", title_path),
                    app.term_colors.dir_style
                ),
                {
                    let current_file = app.get_file_saver();
                    let file = if let Some(current_file) = current_file {
                        current_file.name.to_owned()
                    } else {
                        String::new()
                    };
                    Span::styled(file, app.term_colors.file_style)
                        .add_modifier(Modifier::BOLD)
                }
            ]))
        .block(computer_info_block);

    check_app_error(app);

    let item_info_block = Block::default();
    let item_num_info = title_right
        .alignment(Alignment::Right)
        .block(item_info_block);

    // Attached shell view. The shell menu opened from a shell with `C-s t`
    // keeps that shell's screen behind it (`shell_menu_origin`).
    let menu_shell = if menu_open {
        app.shell_menu_origin
    } else {
        None
    };

    let shell_background = app.selected_block.shell_id().or(menu_shell);

    if let Some(id) = shell_background {
        frame.render_widget(computer_info, title_layout[0]);
        frame.render_widget(item_num_info, title_layout[1]);

        if app.selected_block.is_shell() {
            // Mouse coordinates are translated against this rectangle.
            app.shell_area = chunks[1];
        }

        // The child's cursor is only drawn for the attached view, never over a
        // menu box.
        shell_block::render_shell(app, frame, chunks[1], id, !menu_open)?;

        if menu_open {
            menu_block::render_menu(app, frame, chunks[1]);
            return Ok(())
        }

        render_command_line(app, frame, chunks[2]);

        return Ok(());
    }

    // Expanded Commandline: the full-page editor (`Alt-Ctrl-Tab`). A menu is
    // not an editor: it is drawn later, over the browser.
    if full_page {
        let command_block = Block::default();
        if let cutils::Block::CommandLine(ref msg, cursor) = app.selected_block {
            let command_errors = get_command_line_style(
                app,
                msg,
                cursor
            ).block(command_block);

            frame.render_widget(computer_info, chunks[0]);
            frame.render_widget(command_errors, chunks[1]);
            render_completion(app, frame, chunks[1]);
            return Ok(())
        }

        // The expanded state belongs to a command-line block; if it outlived
        // one, drop it so the next frame draws the browser, not a missing band.
        app.quit_command_mode();

        return Ok(())
    }


    // The page behind a menu or a typed command line is still the browser the
    // user was in, and its layout follows the directory rather than the block
    // which owns the input, so root is recognized by the path.
    let root = app.root();
    let constraints = if root {
        vec![
            Constraint::Percentage(50),
            Constraint::Percentage(50)
        ]
    } else if app.edit_mode.enabled {
        vec![
            Constraint::Percentage(15),
            Constraint::Percentage(60),
            Constraint::Percentage(25)
        ]
    } else {
        vec![
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(40)
        ]
    };

    let browser_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(chunks[1]);

    frame.render_widget(computer_info, title_layout[0]);
    frame.render_widget(item_num_info, title_layout[1]);

    render_parent(app, frame, browser_layout[0]);
    
    // Root has two columns: the preview fills the second when there is one,
    // otherwise the current directory does.
    if root {
        if app.file_content.is_some() {
            render_file(frame, app, browser_layout[1])?;

            finish_browser(app, frame, chunks[1], chunks[2], menu_open);

            return Ok(())
        }
    } else if app.file_content.is_some() {
        render_file(frame, app, browser_layout[2])?;
    } else {
        render_child(app, frame, browser_layout[2]);
    }

    render_current(app, frame, browser_layout[1]);

    finish_browser(app, frame, chunks[1], chunks[2], menu_open);

    Ok(())
}

/// Draw the frame's overlay: the menu, or the command line and its popup.
///
/// A menu leaves the bottom line empty, like the expanded page did before it.
fn finish_browser(
    app: &mut App,
    frame: &mut Frame,
    browser: Rect,
    command_line: Rect,
    menu_open: bool
) {
    if menu_open {
        menu_block::render_menu(app, frame, browser);
        return
    }

    render_command_line(app, frame, command_line);
    render_completion(app, frame, command_line);
}

fn check_app_error(app: &mut App) {
    use cutils::Block as SBlock;

    if !app.app_error.is_empty() {
        let err_msg = app.app_error.to_string();

        // The shell view must not be replaced by the command line: show the
        // error in its status line instead.
        if app.selected_block.is_shell() {
            app.shell_notice = Some(err_msg);
            app.app_error.clear();
            return ()
        }

        // A picker draws the error below its entries; its text body is only a
        // fallback, so the message is not appended there as well.
        let picker_notice = matches!(
            app.selected_block,
            SBlock::CommandLine(_, CursorPos::None)
        ) && app
            .switch_case
            .as_mut()
            .is_some_and(|case| case.append_notice(err_msg.clone()));

        if picker_notice {
            app.command_error = true;
            app.app_error.clear();
            return ()
        }

        if let SBlock::CommandLine(ref mut _msg, ref mut _cursor) = app.selected_block {
            if *_cursor != CursorPos::None {
                app.command_history.push(_msg.get_str().to_owned());
                *_cursor = CursorPos::None;

                *_msg = CmdContent::String(err_msg);
            } else {
                match *_msg {
                    CmdContent::String(ref mut messages) => {
                        messages.push_str(&format!("\n{}", err_msg));
                    },
                    CmdContent::Text(ref mut text) => {
                        text.push_line(err_msg);
                    },
                }
            }
        } else {
            app.selected_block = SBlock::CommandLine(
                CmdContent::String(err_msg),
                CursorPos::None
            );
        }

        app.command_error = true;
        app.app_error.clear();
    }
}

#[inline(always)]
fn get_title_right_info(app: &App) -> (Paragraph<'static>, usize) {
    let current_tab = app.tab_list.current_tab();

    let item_index = if let Some(id) = app.selected_block.shell_id() {
        let pos = current_tab
            .shells
            .iter()
            .position(|shell| *shell == id)
            .map(|idx| idx + 1)
            .unwrap_or(0);

        format!("SH {}/{}", pos, current_tab.shells.len())
    } else if app.path.to_string_lossy() == "/" {
        format!(
            "{}/{}",
            app.selected_item.parent_selected().unwrap() + 1,
            app.parent_files.len()
        )
    } else {
        let current_idx = app.selected_item.current.selected();
        if app.current_files.is_empty() || current_idx.is_none() {
            String::new()
        } else {
            format!(
                "{}/{}",
                current_idx.unwrap() + 1,
                app.current_files.len()
            )
        }
    };

    let shell_badge = if current_tab.shells.is_empty() {
        String::new()
    } else {
        format!(":{}", current_tab.shells.len())
    };

    let tab_index = format!(
        "|{}{}{} ",
        app.tab_list.current() + 1,
        shell_badge,
        if app.tab_list.current() == app.tab_list.len() - 1 {
            "|"
        } else {
            ">"
        }
    );

    let length = item_index.len() + tab_index.len();
    let mut line = Line::default();
    line.push_span(Span::raw(tab_index).bold());
    line.push_span(Span::raw(item_index));

    (Paragraph::new(line), length)
}

fn short_display_path(app: &App) -> String {
    let path = app.path.to_string_lossy();
    let file = if let Some(file_saver) = app.get_file_saver() {
        &file_saver.name
    } else {
        ""
    };

    if path.len() + file.len() <= 68 {
        return path.into()
    }

    let mut splited_path: Vec<&str> = path.split("/").collect();
    splited_path.remove(0);     // The leading `/` produces an empty part.

    for i in 0..(splited_path.len() - 1) {
        splited_path[i] = splited_path[i].get(0..1).unwrap();
    }

    let shorted_path = splited_path.join("/");

    format!("/{}", shorted_path)
}

/// The path shown in the title bar, with exactly one trailing separator.
///
/// `app.path` may still carry a trailing separator (read from the temp file,
/// an old tab state, ...), and the file name is appended right after this
/// string, so a blind extra `/` would render `/home//name`.
fn title_bar_path(app: &App) -> String {
    let path = short_display_path(app);

    if path == "/" {
        return path
    }

    let path = path.trim_end_matches('/');

    if path.is_empty() {
        String::from("/")
    } else {
        format!("{path}/")
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_title_bar_path_has_exactly_one_trailing_separator() {
        let mut app = App::default();

        app.goto_dir("/", Some(false)).unwrap();
        assert_eq!(title_bar_path(&app), "/");

        // A normal directory, then the same path with trailing separators.
        let dir = std::env::temp_dir().join(format!(
            "hire-title-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        app.goto_dir(&dir, Some(false)).unwrap();
        let plain = dir.to_string_lossy().into_owned();

        app.path = PathBuf::from(&plain);
        assert_eq!(title_bar_path(&app), format!("{plain}/"));

        app.path = PathBuf::from(format!("{plain}/"));
        assert_eq!(title_bar_path(&app), format!("{plain}/"));

        app.path = PathBuf::from(format!("{plain}///"));
        assert_eq!(title_bar_path(&app), format!("{plain}/"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A picker shows an error below its entries instead of appending it to
    /// the command-line text, which the picker does not draw.
    #[test]
    fn test_error_of_a_picker_is_shown_in_its_footer() {
        use crate::error::AppResult;
        use crate::key_event::{
            MenuList, MenuPresentation, SwitchCase, SwitchCaseData,
        };

        fn stub(_: &mut App, _: char, _: SwitchCaseData) -> AppResult<bool> {
            Ok(true)
        }

        let mut app = App::default();
        SwitchCase::new(
            &mut app,
            stub,
            MenuPresentation::Picker,
            "Test",
            MenuList::new().entry("1", Line::raw("first")),
            SwitchCaseData::None
        );

        app.app_error.add_error(anyhow::anyhow!("cannot attach"));
        check_app_error(&mut app);

        let case = app.switch_case.as_ref().expect("the menu stays open");
        let footer = case.list().expect("a picker").footer_lines();
        let text: String = footer
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        assert!(text.contains("cannot attach"), "{text}");
        assert!(app.command_error);
        assert!(app.app_error.is_empty());
    }

    /// A dialog still keeps its error in the text it draws itself.
    #[test]
    fn test_error_of_a_dialog_stays_in_its_text() {
        use crate::error::AppResult;
        use crate::key_event::{MenuPresentation, SwitchCase, SwitchCaseData};

        fn stub(_: &mut App, _: char, _: SwitchCaseData) -> AppResult<bool> {
            Ok(true)
        }

        let mut app = App::default();
        SwitchCase::new(
            &mut app,
            stub,
            MenuPresentation::Dialog,
            "Test",
            CmdContent::String(String::from("base")),
            SwitchCaseData::None
        );

        app.app_error.add_error(anyhow::anyhow!("boom"));
        check_app_error(&mut app);

        let cutils::Block::CommandLine(CmdContent::String(text), _) = &app.selected_block else {
            panic!("the dialog text is still in the block")
        };

        assert!(text.contains("boom"), "{text}");
        assert!(app.command_error);
        assert!(app.app_error.is_empty());
    }
}
