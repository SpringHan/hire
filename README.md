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

## Usage

You can get usage by:

```shell
hire --help
```

## TODO

- [x] Image preview support
- [x] Storage of specific tabs
- [x] Keymap config support
- [x] Optimize the editing ability for command line
- [x] Add Edit mode for more convenient file editing
- [ ] Bottom hint for whether current shell process is run by hire
- [ ] Refactor dir tree
- [ ] Highlight for file preview

## LICENSE
MIT
