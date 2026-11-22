// Command type.

use anyhow::bail;

use crate::{option_get, utils::Direction};

#[derive(Clone)]
pub enum AppCommand {
    Tab,
    Goto,
    Shell,
    Paste,
    Delete,
    Search,
    Refresh,
    NextTab,
    PrevTab,
    FzfJump,
    VimDiff,
    GotoFile,
    CmdShell,
    ShellMenu,

    /// Attach the shell the current tab used last (`Tab::active_shell`).
    ShellAttach,

    RgSearch,
    EditMode,
    PrevPath,
    MacroPage,
    CreateDir,
    CreateFile,
    GotoBottom,
    HideOrShow,
    OutputFile,
    MarkExpand,
    PathHistory,
    ShowNaviIndex,
    SingleSymlink,
    PrintFullPath,
    QuitAfterOutput,

    /// Enter the command line block with a leading `:` so the user can type a
    /// hire command (the keymap's `enter_command_line`).
    EnterCommandLine,

    /// Re-read `auto_config.toml`, `user_config.toml` and `keymap.toml` and
    /// apply them to the running process.
    ReloadConfig,

    /// Run a root instance of hire with the tabs of the current session.
    SudoSession,

    /// When the boolean is true, only mark single file.
    Mark(bool),

    /// Switch tab with index.
    SwitchTab(char),

    /// When the boolean is true, scroll down.
    ListScroll(bool),

    /// Search the text preview; like [`Self::Search`] it opens the command line,
    /// with a leading `?` instead of `/`.
    PreviewSearch,

    /// Page the text preview, down when the boolean is true.
    PreviewScrollPage(bool),

    /// Scroll the text preview by one line, down when the boolean is true.
    PreviewScrollLine(bool),

    /// Scroll the text preview by one column, right when the boolean is true.
    PreviewScrollColumn(bool),

    /// The command to insert navigation index.
    /// Element of it is used to imply what the index number is.
    NaviIndexInput(u8),

    /// When boolean value is true, the cursor will be moved to the edge.
    AppendFsName(bool),
    
    /// Move cursor to the candidate, jumping to the next when the boolean is true.
    MoveCandidate(bool),

    /// When the boolean value is true, set the working directory;
    /// otherwise jump to the working directory.
    WorkDirectory(bool),


    /// The value of it is the direction for movement,
    ItemMove(Direction),

    /// The first element is the shell command with its arguments,
    /// the second element refers to whether refreshing showing file items.
    ShellCommand(Vec<String>, bool),

    /// Attach the shell pinned with `C-s *`.
    AttachPinned,

    // Edit Mode
    QuitEdit,
    EditDelete,
    EditGotoTop,
    EditGotoBottom,

    /// When the value is true, create a directory,
    /// otherwise create a file.
    EditNew(bool),

    /// When the boolean is true, only mark single file.
    EditMark(bool),

    /// When the boolean value is true, insert at the end.
    /// Otherwise insert at the beginning.
    EditInsert(bool),

    /// Enter insert mode with the cursor right before the file extension,
    /// which is the edit-mode counterpart of [`Self::AppendFsName`].
    EditAppendFsName,

    /// When the value is true, select next item, otherwise the previous one.
    EditMoveItem(bool),

    /// When the value is true, scroll down, otherwise scroll up.
    EditListScroll(bool),
}

impl AppCommand {
    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        let command_err = "Unknow command for binding";
        let command_slice = value.split(" ")
            .collect::<Vec<_>>();

        let cmd_arg = command_slice.get(1);
        let command = match *option_get!(command_slice.get(0), command_err) {
            "tab_operation"     => Self::Tab,
            "goto_operation"    => Self::Goto,
            "spawn_shell"       => Self::Shell,
            "paste_operation"   => Self::Paste,
            "delete_operation"  => Self::Delete,
            "search"            => Self::Search,
            "fzf_jump"          => Self::FzfJump,
            "vim_diff"          => Self::VimDiff,
            "refresh"           => Self::Refresh,
            "next_tab"          => Self::NextTab,
            "prev_tab"          => Self::PrevTab,
            "cmdline_shell"     => Self::CmdShell,
            "shell_menu"        => Self::ShellMenu,
            "attach_shell"      => Self::ShellAttach,
            "attach_pinned"     => Self::AttachPinned,
            "goto_file"         => Self::GotoFile,
            "prev_path"         => Self::PrevPath,
            "edit_mode"         => Self::EditMode,
            "rg_search"         => Self::RgSearch,
            "create_dir"        => Self::CreateDir,
            "macro_page"        => Self::MacroPage,
            "create_file"       => Self::CreateFile,
            "goto_bottom"       => Self::GotoBottom,
            "hide_or_show"      => Self::HideOrShow,
            "mark_expand"       => Self::MarkExpand,
            "output_file"       => Self::OutputFile,
            "path_history"      => Self::PathHistory,
            "sudo_session"      => Self::SudoSession,
            "full_path"         => Self::PrintFullPath,
            "single_symlink"    => Self::SingleSymlink,
            "show_navi_index"   => Self::ShowNaviIndex,
            "enter_command_line" => Self::EnterCommandLine,
            "quit_after_output" => Self::QuitAfterOutput,
            "reload_config"     => Self::ReloadConfig,

            "list_scroll" => Self::ListScroll(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "preview_search" => Self::PreviewSearch,

            "preview_scroll_page" => Self::PreviewScrollPage(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "preview_scroll_line" => Self::PreviewScrollLine(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "preview_scroll_column" => Self::PreviewScrollColumn(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "move" => Self::ItemMove(Direction::from_str(
                option_get!(cmd_arg, command_err)
            )?),

            "move_candidate" => Self::MoveCandidate(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "mark" => Self::Mark(
                *option_get!(cmd_arg, command_err) == "single"
            ),

            "work_directory" => Self::WorkDirectory(
                *option_get!(cmd_arg, command_err) == "set"
            ),

            "append_filename" => Self::AppendFsName(
                *option_get!(cmd_arg, command_err) == "extension"
            ),

            "navi_input" => Self::NaviIndexInput(
                option_get!(cmd_arg, command_err).parse::<u8>()?
            ),

            "shell_command" => {
                let refresh = *option_get!(cmd_arg, command_err) == "true";
                let command_vec = command_slice[2..].into_iter()
                    .map(|_str| (*_str).to_owned())
                    .collect::<Vec<_>>();

                Self::ShellCommand(command_vec, refresh)
            },

            "switch_tab" => {
                let _str = *option_get!(cmd_arg, command_err);
                if _str.len() < 1 {
                    bail!("{}", command_err)
                } else {
                    Self::SwitchTab(_str.chars().next().unwrap())
                }
            },

            // Edit Mode
            "quit_edit"   => Self::QuitEdit,
            "edit_delete" => Self::EditDelete,
            "edit_top"    => Self::EditGotoTop,
            "edit_bottom" => Self::EditGotoBottom,

            "edit_new" => Self::EditNew(
                *option_get!(cmd_arg, command_err) == "dir"
            ),

            "edit_mark" => Self::EditMark(
                *option_get!(cmd_arg, command_err) == "single"
            ),

            "edit_move" => Self::EditMoveItem(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            "edit_insert" => Self::EditInsert(
                *option_get!(cmd_arg, command_err) == "end"
            ),

            "edit_append_filename" => Self::EditAppendFsName,

            "edit_list_scroll" => Self::EditListScroll(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            _ => bail!("Unknow command for keybinding")
        };

        Ok(command)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// The shipped keymaps bind `B` to this command, so the string has to stay
    /// parseable.
    #[test]
    fn test_attach_shell_command() {
        assert!(matches!(
            AppCommand::from_str("attach_shell").unwrap(),
            AppCommand::ShellAttach
        ));
    }

    /// `reload_config` re-reads the three config files.
    #[test]
    fn test_reload_config_command() {
        assert!(matches!(
            AppCommand::from_str("reload_config").unwrap(),
            AppCommand::ReloadConfig
        ));
    }

    /// The shipped keymaps bind `:` to this command.
    #[test]
    fn test_enter_command_line_command() {
        assert!(matches!(
            AppCommand::from_str("enter_command_line").unwrap(),
            AppCommand::EnterCommandLine
        ));
    }

    /// The shipped keymaps bind `*` to this command.
    #[test]
    fn test_attach_pinned_command() {
        assert!(matches!(
            AppCommand::from_str("attach_pinned").unwrap(),
            AppCommand::AttachPinned
        ));
    }

    /// The edit-mode keymaps bind `a` to this command; unlike
    /// `append_filename` it takes no argument, because `edit_insert` already
    /// covers the beginning and the end of the name.
    #[test]
    fn test_edit_append_filename_command() {
        assert!(matches!(
            AppCommand::from_str("edit_append_filename").unwrap(),
            AppCommand::EditAppendFsName
        ));
    }

    /// The text preview commands ship without a key, but the strings a user
    /// writes into `keymap.toml` are fixed by `from_str`.
    #[test]
    fn test_preview_commands() {
        assert!(matches!(
            AppCommand::from_str("preview_search").unwrap(),
            AppCommand::PreviewSearch
        ));

        assert!(matches!(
            AppCommand::from_str("preview_scroll_page next").unwrap(),
            AppCommand::PreviewScrollPage(true)
        ));
        assert!(matches!(
            AppCommand::from_str("preview_scroll_page prev").unwrap(),
            AppCommand::PreviewScrollPage(false)
        ));

        assert!(matches!(
            AppCommand::from_str("preview_scroll_line next").unwrap(),
            AppCommand::PreviewScrollLine(true)
        ));
        assert!(matches!(
            AppCommand::from_str("preview_scroll_line prev").unwrap(),
            AppCommand::PreviewScrollLine(false)
        ));

        assert!(matches!(
            AppCommand::from_str("preview_scroll_column next").unwrap(),
            AppCommand::PreviewScrollColumn(true)
        ));
        assert!(matches!(
            AppCommand::from_str("preview_scroll_column prev").unwrap(),
            AppCommand::PreviewScrollColumn(false)
        ));

        // The direction argument is mandatory, like the other scroll commands.
        assert!(AppCommand::from_str("preview_scroll_page").is_err());
        assert!(AppCommand::from_str("preview_scroll_line").is_err());
        assert!(AppCommand::from_str("preview_scroll_column").is_err());
    }
}
