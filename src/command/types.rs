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
    CreateDir,
    CreateFile,
    GotoBottom,
    HideOrShow,
    SingleSymlink,
    PrintFullPath,

    /// When the boolean is true, only mark single file.
    Mark(bool),

    /// When boolean value is true, the cursor will be moved to the edge.
    AppendFsName(bool),

    /// Move cursor to the candidate, jumping to the next when the boolean is true.
    MoveCandidate(bool),

    /// When the boolean value is true, set the working directory;
    /// otherwise jump to the working directory.
    WorkDirectory(bool),

    /// The first element is the direction for movement,
    /// and the second element refers to whether move to the edeg.
    ItemMove(Direction),

    /// The first element is the shell command with its arguments,
    /// the second element refers to whether refreshing showing file items.
    ShellCommand(Vec<String>, bool),
}

impl AppCommand {
    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        let command_err = "Unknow command for binding";
        let command_slice = value.split(" ")
            .collect::<Vec<_>>();

        let cmd_arg = command_slice.get(1);
        let command = match *option_get!(command_slice.get(0), command_err) {
            "tab_operation"    => Self::Tab,
            "goto_operation"   => Self::Goto,
            "spawn_shell"      => Self::Shell,
            "paste_operation"  => Self::Paste,
            "delete_operation" => Self::Delete,
            "search"           => Self::Search,
            "fzf_jump"         => Self::FzfJump,
            "refresh"          => Self::Refresh,
            "cmdline_shell"    => Self::CmdShell,
            "create_dir"       => Self::CreateDir,
            "create_file"      => Self::CreateFile,
            "goto_bottom"      => Self::GotoBottom,
            "hide_or_show"     => Self::HideOrShow,
            "full_path"        => Self::PrintFullPath,
            "single_symlink"   => Self::SingleSymlink,

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

            "shell_command" => {
                let refresh = *option_get!(cmd_arg, command_err) == "true";
                let command_vec = command_slice[2..].into_iter()
                    .map(|_str| (*_str).to_owned())
                    .collect::<Vec<_>>();

                Self::ShellCommand(command_vec, refresh)
            },
            _ => bail!("Unknow command for keybinding")
        };

        Ok(command)
    }
}
