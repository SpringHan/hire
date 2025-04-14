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
    FzfJump,
    CmdShell,
    EditMode,
    CreateDir,
    CreateFile,
    GotoBottom,
    HideOrShow,
    OutputFile,
    MarkExpand,
    ShowNaviIndex,
    SingleSymlink,
    PrintFullPath,
    CommandInsert,
    QuitAfterOutput,

    /// When the boolean is true, only mark single file.
    Mark(bool),

    /// When the boolean is true, scroll down.
    ListScroll(bool),

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
            "refresh"           => Self::Refresh,
            "cmdline_shell"     => Self::CmdShell,
            "edit_mode"         => Self::EditMode,
            "create_dir"        => Self::CreateDir,
            "create_file"       => Self::CreateFile,
            "goto_bottom"       => Self::GotoBottom,
            "hide_or_show"      => Self::HideOrShow,
            "mark_expand"       => Self::MarkExpand,
            "output_file"       => Self::OutputFile,
            "full_path"         => Self::PrintFullPath,
            "single_symlink"    => Self::SingleSymlink,
            "show_navi_index"   => Self::ShowNaviIndex,
            "command_insert"    => Self::CommandInsert,
            "quit_after_output" => Self::QuitAfterOutput,

            "move" => Self::ItemMove(Direction::from_str(
                option_get!(cmd_arg, command_err)
            )?),

            "list_scroll" => Self::ListScroll(
                *option_get!(cmd_arg, command_err) == "next"
            ),

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

            "edit_list_scroll" => Self::EditListScroll(
                *option_get!(cmd_arg, command_err) == "next"
            ),

            _ => bail!("Unknow command for keybinding")
        };

        Ok(command)
    }
}
