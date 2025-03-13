// Command type.

use std::borrow::Cow;

use crate::utils::Direction;

pub enum AppCommand<'a> {
    Tab,
    Goto,
    Shell,
    Paste,
    Delete,
    Search,
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
    ShellCommand(Vec<Cow<'a, str>>, bool),
}
