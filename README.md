# Hire - ligHtweight fIle bRowsEr

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

Notice: The `auto_config.toml` is generated & edited by app.

### user_config.toml

This file contains user-specific configuration options. Here are the available settings:

```toml
# Default shell to use for commands
default_shell = "bash"

# GUI commands to show in the interface to avoid a required key press.
gui_commands = ["lazygit", "vim"]

# Program to use for reading files, such as vim, cat, bat, etc.
file_read_program = "vim"
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
   - Insert text at beginning/end of filenames (`H`/`h` in Colemak, `I`/`i` in QWERTY)

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

## TODO

- [x] Image preview support
- [x] Storage of specific tabs
- [x] Keymap config support
- [x] Optimize the editing ability for command line
- [x] Add Edit mode for more convenient file editing
- [ ] Refactor the project
- [ ] Highlight for file preview
- [ ] Migrate to Rust 2024 Edition

## LICENSE
MIT
