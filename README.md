# Hire - ligHtweight fIle bRowsEr

[![CI](https://github.com/SpringHan/hire/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/SpringHan/hire/actions/workflows/ci.yml)

This is a repository which contains a terminal file browser written in Rust.

I made the project with the aim for practicing Rust and this tool was almost created accroding to my own situations and demands.

## Installation

```shell
./install.sh --target ~/.local/bin/hire

# Build with colemak keyboard layout
./install.sh --keymap colemak.toml --target ~/.local/bin/hire
```

## Configuration

There're three config files for hire: `auto_config.toml`, `user_config.toml` and `keymap.toml`.

By default they live in `~/.config/springhan/hire/`. Set the `HIRE_CONFIG_DIR`
environment variable to use another directory, e.g.
`HIRE_CONFIG_DIR=/tmp/hire-config hire`; this is handy to run the app (or tests)
with a configuration that is independent from your own, so commands such as
`gui_file_open_cmd` are never triggered by accident.

When developing hire itself, `src/config/mod.rs` has a deliberate toggle which
switches the file names to `auto_config_dev.toml`, `user_config_dev.toml` and
`keymap_dev.toml`, so a new feature can be tested without touching the real
configuration. That is a developer-only local state; normal builds read the
production names above. See `docs/ARCHITECTURE.md` §6 for details.

Notice: The `auto_config.toml` is generated & edited by app.

### Colors

File colors come from the `LS_COLORS` environment variable, like `ls --color`.
When it is not set (or is empty), hire uses a built-in `dircolors` palette
shipped in `src/app/default_ls_colors.txt`, so colours stay readable in CI, a
bare shell or a minimal container.

### user_config.toml

This file contains user-specific configuration options. Here are the available settings:

```toml
# Default shell to use for commands
default_shell = "bash"

# GUI commands to show in the interface to avoid a required key press.
gui_commands = ["lazygit", "vim"]

# Program to open text files with, such as vim, cat, bat, etc.
# Files which are not text (pictures, PDFs, archives, …) are opened with `xdg-open`.
file_read_program = "vim"

# Command of a GUI program to open the output file/directory, such as xdg-open/emacs.
# When it's empty, the path will be written into the output file instead.
gui_file_open_cmd = "emacsclient -n"
```

### keymap.toml

This file defines key bindings for the application.  
Defaultly, the `keymap.toml` will be copied to `~/.config/springhan/hire` folder after running `install.sh`.

The format is:

```toml
keymap = [
    # ... other keybindings ...

    { key = "k", run = "command" }
]

# Define custom shell commands that can be executed with a key binding
# Format: "shell_command *Whether refresh displaying files after command* *command*"
# Use "$." to substitute the currently selected file/directory
# E.g.
keymap = [
    # ... other keybindings ...

    { key = "v", run = "shell_command true vim $." }
]
```

#### Shell scroll keys

Two extra arrays bind the keys of the attached shell view (see "Shell Sessions").
Unlike `keymap`, their `key` values are full key specs: a single character, an
optional modifier (`C-`, `M-`, `S-`) and a named key (`Up`, `PageUp`, `Esc`,
`Enter`, `F5`, …), e.g. `"M-u"`, `"C-b"`, `"PageUp"`.

```toml
# Keys which scroll the shell history instead of being sent to the shell.
shell_scroll = [
    { key = "M-u", run = "scroll up"   },
    { key = "M-e", run = "scroll down" },
]

# Keys of the scroll mode (`C-s [`).
shell_scroll_mode = [
    { key = "k",        run = "scroll up"        },
    { key = "j",        run = "scroll down"      },
    { key = "PageUp",   run = "scroll page up"   },
    { key = "PageDown", run = "scroll page down" },
    { key = "g",        run = "scroll top"       },
    { key = "G",        run = "scroll bottom"    },
    { key = "q",        run = "scroll quit"      },
]
```

The commands are `scroll up`, `scroll down`, `scroll page up`,
`scroll page down`, `scroll top`, `scroll bottom` and `scroll quit`. The
defaults listed above (plus the arrow keys, `C-p`/`C-n`/`C-b`/`C-f`,
`Home`/`End`, `Esc` and `Enter`) stay in place when a section is missing; an
entry for a key which already has a default replaces that binding, so a single
key can be remapped without repeating the whole list.

## Shell arguments

You can get the arguments by:

```shell
hire --help
```

## Special features

### Edit Mode

Edit Mode provides an enhanced interface for batch file operations. When activated (with `edit_mode` command), it allows you to:

1. **Navigation**:
   - Move between files with `u`/`e` (Colemak) or `k`/`j` (QWERTY)
   - Jump to top/bottom with `g`/`G`
   - Scroll list with `v`/`V`

2. **File Operations**:
   - Create new files/directories (`k`/`K` in Colemak, `n`/`N` in QWERTY)
   - Mark files with `delete` sign (`d`)
   - Mark files for batch operations:
     - `m` - Mark/unmark all files
     - `Space` - Mark/unmark single file

3. **Text Editing**:
   - Insert text at beginning/end of filenames (`H`/`A` in Colemak, `I`/`A` in QWERTY)
   - Move to the next/previous word: `Alt-f` / `Alt-b`; delete a whole word before/after the cursor: `C-Backspace` / `Alt-d`; delete the character after the cursor: `C-d`

4. **Exiting**:
   - `Enter` - Apply edited content to current path
   - `Q`/`Esc` - Quit edit mode

### Output File

The Output File feature allows you to write file/directory paths to a specified output file. This is useful for integration with other tools such as your editor. It's a really convenient way to get target file path with hire.

Key behaviors:
- When enabled, writes either:
  - The full path of the currently selected file (if one is selected and you pressed key of `move right` command)
  - The current directory path (if you pressed `Enter` in a path)
- Automatically quits the application after writing

This feature should be used with `--output-file` argument:

```bash
hire --output-file /tmp/hire_output.txt
```

### Working Directory

Hire remembers a working directory, which is shared between runs:

- `W` (`work_directory set`) - set the current directory as the working directory
- `w` (`work_directory goto`) - jump to the working directory

Whenever a working directory has been set, its path is shown on the right side of the state line (long paths are abbreviated).

### Tabs

`tab_operation` (default `t`) opens the tab menu:

| Key | Action |
| --- | --- |
| `n` | Create a new tab |
| `f` / `b` | Switch to the next / previous tab |
| `c` | Close the current tab |
| `d` then a number | Delete the tab with that number |
| `o` | Close every other tab |
| `s` then a number | Open a stored tab group |
| `S` | Store the currently open tabs as a group |

Removing a tab kills the shell sessions mounted on it, so whenever a mounted shell would be lost — closing the current tab (`c`), deleting a tab by number (`d`), closing the other tabs (`o`) or restoring a stored group (`s`) — hire asks first and only acts on `y`; any other key returns to the tab menu.

### Shell Sessions

`spawn_shell` (default `S`) opens a shell that hire owns and shows it in the current tab. The shell is a real PTY session: it keeps running after it is detached, and one tab can hold several of them (like tmux windows).

- `S` — create a new shell on the current tab and attach it.
- `T` (`shell_menu`) — list the shells which can be attached: the current tab's shells first (their numbers are unchanged, starting at `0`), then the shells of the other tabs grouped under `Tab N`. Digits attach one, `n` creates a new shell on the current tab, `x` followed by a number kills one, and `D` kills every shell of the current tab (the tab stays open; press `y` to confirm). `x` and `D` only ever touch the current tab.
- Attaching a shell of another tab **borrows** it: hire switches to that tab internally (the title bar shows it while the shell is in front) and, on detach, refreshes the shell's own tab and then returns you to the tab you were on. So `C-s d` puts you back where you started. The prefix commands keep working on the shell's own tab (`C-s c` opens there, `C-s n/p` cycle its shells), because that is the tab the view belongs to.
- `B` (`attach_shell`) — go straight back to the shell this tab used last, without opening the shell menu. The tab remembers the shell which was attached last (`Tab::active_shell`), however it was chosen (`S`, the menu, `C-s n`/`C-s p` or `C-s <number>`), so it follows you across detach/attach. On a tab without a shell, `B` borrows the shell of another tab which was attached most recently, with the same "refresh and return" behaviour; an error is shown only when no other tab has a shell either.
- The shell menu and the attached shell's status line name the program **currently running** in the shell (`vim`, `htop`, `nvim`, …), not just the shell program it was started with. A shell sitting at its prompt shows its own name (`bash`, `zsh`), so several shells started with the same `default_shell` can be told apart at a glance.
- While a shell is attached every key is forwarded to it; the prefix chord `C-s` (`shell_prefix`) gives hire a command. The status line only lists the prefix commands after `C-s` has been pressed, so the bottom of the screen stays clean while the shell is used normally:

| Key | Action |
| --- | --- |
| `C-s d` | Detach: the file browser comes back, the shell keeps running on the tab |
| `C-s n` / `C-s p` | Attach the next / previous shell of the tab |
| `C-s <number>` | Attach the shell with that number (0-based) |
| `C-s c` | Create a new shell on this tab and attach it |
| `C-s x` | Kill the attached shell (press `y` to confirm) |
| `C-s D` | Kill every shell mounted on the current tab (press `y` to confirm) |
| `C-s o` | Kill every **other** shell of the tab and keep the attached one (press `y` to confirm) |
| `C-s [` | Enter the scroll mode: browse the shell's output history |
| `C-s C-s` | Send a literal `C-s` to the shell |

- **Scrollback.** Commands which print more than one screen (`ls -la`, `tree`, build logs, …) stay readable:
  - `C-s [` enters the scroll mode. The status line then shows `SCROLL <n>` and the configured keys; `PageUp`/`PageDown` (or `C-b`/`C-f`) page, `k`/`j` (or `↑`/`↓`, `C-p`/`C-n`) move one line, `g`/`Home` jumps to the oldest line and `G`/`End` returns to the bottom. `q`, `Esc` or `Enter` also return to the live screen. Scrolling back to the bottom leaves the mode automatically.
  - The prefix chord still works while scrolled: `C-s` leaves the mode and arms the prefix, so `C-s d`, `C-s x`, … behave as usual.
  - Without entering the mode, `Alt+u` scrolls up and `Alt+e` scrolls down. Scrolling up enters the mode (so the keys no longer reach the shell); scrolling down to the bottom leaves it.
  - Both key sets are configurable in `keymap.toml` (`shell_scroll` and `shell_scroll_mode`, see below); the defaults are `M-u`/`M-e` and the keys listed above. The status-line hint is built from the configured bindings.
- Mouse and pasted text:
  - **The wheel belongs to the program in the shell by default.** When it enabled mouse tracking (`vim`, `less`, `htop`, …) hire forwards wheel, click and drag events to it, so its own mouse scrolling keeps working; when it did not, hire does not capture the mouse at all, so the terminal's native wheel and drag-selection work as usual (no `Shift` needed).
  - **`Shift` + wheel is hire's**: it enters the scroll mode (or scrolls it when it is already open). This needs hire to receive the wheel, i.e. either the program enabled mouse tracking or the scroll mode is open; with no mouse reporting the terminal handles the wheel itself, so use `C-s [` or `Alt+u` instead.
  - **In the scroll mode hire captures the mouse**, so the wheel scrolls the history freely regardless of the program's mouse mode; leaving the mode (or reaching the bottom) releases the mouse again. Drag-selection needs `Shift` only while a program inside the shell has asked for the mouse (the terminal's usual override).
  - When the program enables bracketed paste (`ESC[?2004h`), a paste arrives as a single bracketed paste, preserving newlines and CJK text. A paste while the scroll mode is open is ignored (press `q` first).
  - Focus events are not forwarded.

- Numbers are 0-based and may have more than one digit: with 11 shells, `C-s 1 0` attaches shell 10. A partial number is committed on `Enter`, when no further digit could still be valid, or after a short timeout; the status line shows the digits while they are being typed.
- The title bar shows how many shells the current tab has (`|1:2|` means tab 1 has two shells) and the attached shell's position (`SH 2/3`).
- Leaving the shell view — `C-s d` or the shell exiting — returns to the browser and re-reads the current directory, so files the shell created, renamed or deleted show up immediately (the selected file is kept). The same happens when a shell is killed and there is no shell left to attach.
- Images painted by a TUI inside a mounted shell (for example `yazi` previews) are shown when the terminal speaks the **kitty graphics protocol**: hire answers the program's terminal queries and forwards the image data, and the program's own unicode placeholders place it on screen. Programmes that fall back to sixel/iTerm2 images, and hire running inside tmux, keep the old behaviour (the image is not shown inside the shell).
`default_shell` selects the program, and `shell_prefix` changes the prefix chord:

```toml
default_shell = "bash"
shell_prefix = "C-s"
```

Notes:

- `C-s` is normally XOFF. Hire reads the terminal in raw mode and intercepts the chord before forwarding anything, so the shell never sees it. If you run hire inside tmux, or your terminal's own `C-s` handling interferes, set `shell_prefix` to another chord (`C-a`, `C-o`, `F12`, …).
- Shell sessions live inside the hire process: quitting hire kills them, closing a tab kills the shells mounted on it (the tab menu asks first when shells are mounted), and they are not handed over to a sudo session nor stored in `storage_tabs`.
- `hire --working-directory` opens a mounted shell as well, so it can be detached and the browser can be used without losing it.

### Navigation Index

Navigation Index allows quick jumping to specific items by entering their index number. 

Usage:
- Activated by pressing the navigation index key (default `.`, which is keybinding for `show_navi_index`)
- Press keybindings for `navi_input` to insert index number (you can bind the middle line keys on your keyboard to `navi_input`)
- Press `Enter` to jump to target item or `Esc` to cancel jumping
- Works in both normal mode and edit mode

An example of keymap configuration for navigation index:

```toml
keymap = [
    # Other bindings.....

    # An example for colemak keyboard layout.
    { key = "a", run = "navi_input 1" },
    { key = "r", run = "navi_input 2" },
    { key = "s", run = "navi_input 3" },
    { key = "t", run = "navi_input 4" },
    { key = "d", run = "navi_input 5" },
    { key = "h", run = "navi_input 6" },
    { key = "n", run = "navi_input 7" },
    { key = "e", run = "navi_input 8" },
    { key = "i", run = "navi_input 9" },
    { key = "o", run = "navi_input 0" }
]
```

### Path History

Every jump made with `goto_operation`, `fzf_jump` (and `rg_search`), or `goto_file` records the path you left. At most 30 paths are kept, the newest one is stored at the last place.

- `prev_path` - jump back to the last recorded path
- `path_history` - open the path history page, which shows every recorded path numbered from `01` to `30`; press the number keys of a path to jump to it

An example of keymap configuration for path history:

```toml
keymap = [
    # Other bindings.....

    { key = "b", run = "prev_path"    },  # Jump back to the previous path
    { key = "H", run = "path_history" },  # Open the path history page
]
```

### Root Session

Sometimes root privileges are needed in the middle of a browsing session. `sudo_session` starts a root instance of hire in the same terminal and hands the current tabs over to it, so the root instance opens exactly the tabs you had open:

```toml
keymap = [
    # Other bindings.....

    { key = "Z", run = "sudo_session" },
]
```

- Pressing the key suspends the current (unprivileged) instance, asks `sudo` for the password on the normal screen and starts `sudo -E <hire> --sudo-session <state> --config-dir <dir>`.
- The new instance restores every tab (path, hidden-file flag and the file selected in the tab) and starts in the tab which was focused.
- Do the root work there, then quit it (`q`, and confirm with `y` when tabs are open). The root process exits and the original unprivileged instance comes back exactly where it was: this is how the root privileges are dropped manually.
- The root instance runs with a throw-away copy of `auto_config.toml`, `user_config.toml` and `keymap.toml`. Its configuration changes (new bookmarks, stored tabs) are discarded when it exits, so your real config files are never rewritten as root.
- Changes of tabs made in the root instance are not synced back to the original instance.
- Files created or modified as root keep the root ownership; nothing is chowned automatically.

## Keybindings for command line

- Cursor move left: `C-b`
- Cursor move right: `C-f`
- Cursor move to beginning: `C-a`
- Cursor move to end: `C-e`
- Delete word: `Alt-Backspace` (Also in edit mode)
- Delete the whole word before the cursor: `C-Backspace` (Also in edit mode; also `C-h` / `C-w` on terminals without the kitty keyboard protocol)
- Delete the whole word after the cursor: `Alt-d` (Also in edit mode)
- Cursor move to the end of the next word: `Alt-f` (Also in edit mode)
- Cursor move to the start of the previous word: `Alt-b` (Also in edit mode)
- Delete the character after the cursor: `C-d` (Also in edit mode)
- Expand command line: `Alt-Tab`
- Previous/next completion candidate: `C-p` / `C-n` (also page the expanded command line view and switch prev/next history command)
- Previous/next command history from the browser: `Up` / `Down` (enters the command line and fills the newest / oldest history command)

## TODO

- [x] Image preview support
- [x] Storage of specific tabs
- [x] Keymap config support
- [x] Optimize the editing ability for command line
- [x] Add Edit mode for more convenient file editing
- [x] Mountable shell sessions per tab (tmux-like attach/detach)
- [ ] Refactor the project
- [ ] Highlight for file preview
- [x] Migrate to Rust 2024 Edition

## LICENSE
MIT
