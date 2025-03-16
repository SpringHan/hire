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

## Usage

You can get usage by:

```shell
hire --help
```

## TODO

- [x] Image preview support
- [x] Storage of specific tabs
- [x] Keymap config support
- [ ] Optimize the editing ability for command line
- [ ] Edit multiple files with terminal editor
- [ ] Bottom hint for whether current shell process is run by hire
- [ ] Refactor dir tree
- [ ] Highlight for file preview

## LICENSE
MIT
