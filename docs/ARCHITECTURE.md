# Hire — Architecture

`hire` (ligHtweight fIle bRowsEr) is a terminal file manager written in Rust, in the
spirit of `ranger`. It is a single-binary, keyboard-driven TUI application.

- **Language / edition:** Rust 2024 (`Cargo.toml`, package `hire`).
- **TUI stack:** [`ratatui`](https://crates.io/crates/ratatui) 0.30 with
  `crossterm` as the backend, plus `ratatui-image` 10 for inline image preview
  (built without the default `chafa` feature, so no system library is needed;
  halfblocks fall back to the primitive renderer).
- **Shell sessions:** `portable-pty` 0.9 (PTY, spawning, resize) and `vt100`
  0.16 (parsing the shell's output into a screen buffer, including a scrollback
  that can be paged through). `vt100` 0.16 requires `unicode-width 0.2.1+`,
  which is why ratatui is 0.30 (`ratatui` 0.29 pinned `unicode-width = "=0.2.0"`
  and `vt100` 0.15's scrollback could only show one screen of history).
- **Supporting crates:** `clap` (CLI args), `toml_edit` (config files),
  `thiserror` + `anyhow` (errors), `chrono` (timestamps), `file-size`,
  `copy_dir`, `is_executable`, `image`, `ansi-to-tui`, `lazy_static`, `libc`
  (euid check, `tcgetpgrp` for the foreground program of a shell).

---

## 1. High-level design

Hire is an event-driven application built around one mutable state object, `App`.

### Prcoess
1. CLI args → App::default() → init_config()
2. loop:
   a. execute pending macro
   b. reap exited shell sessions; commit a timed-out shell number; mirror the
      child's mouse/bracketed-paste modes on the outer terminal; answer the
      shells' terminal queries and forward their kitty images to the terminal
   c. clear the terminal if a shell was attached/detached (or an image was
      forwarded: the placeholder cells must be painted after it)
   d. ui::ui(frame, &mut app)
   e. poll/read events (16 ms while a shell is attached, 200 ms otherwise)
   f. handle_event / handle_view_mouse / handle_view_paste
   g. drain search / image channels

### Modules

- `main.rs` — Entrance, start the main loop
- `key_event::handle_event` — Handle key input
  - Input：key event
  - Outupt：AppCommand
  - Function call：AppCommand::execute
  - Exception: while a shell is attached, the key goes to
    `key_event::shell::handle_view_key` (prefix chord or PTY bytes)
- `app::App` — Global state Container
  - Block::Browser | Block::CommandLine | Block::Shell
- `ui::ui` — Only read state of App and render ui
  - Render areas：parent / current / child / file / cmdline
  - Shell view: title / shell screen (`ui/shell_block`) / shell status line

### Data stream

key event → key_event::handle_event → AppCommand → AppCommand::execute → App
                                                                          ↑
ui::ui Read state of App ←──────────────────────────────────────────────┘
                                                          
shell session: PTY bytes → reader thread → vt100 screen → ui::ui (shell view)
               keys → encode_key → PTY master


Key architectural traits:

1. **One central state struct.** `App<'a>` (in `app/mod.rs`) owns everything:
   the current path, the three file lists, selection indices, the active block,
   configuration, tabs, macro state, etc. There is no global state except the
   window height atomic (`utils::WINDOW_HEIGHT`).

2. **Behaviour is attached to `App` from many modules.** The project uses
   "`impl App` block per module" throughout: navigation lives in
   `key_event/cursor_movement`, command-line editing in
   `key_event/command_line.rs`, search in `key_event/file_search.rs`, shell
   helpers in `key_event/shell`, command parsing in `command/cmd_utils.rs`, and
   the core file model in `app/mod.rs`. `app/mod.rs` is the hub that other
   modules extend.

3. **Three block modes.** `utils::Block` is `Browser(bool)` (the file
   browser; the bool means "current directory is `/`"),
   `CommandLine(CmdContent, CursorPos)`, or `Shell(ShellId)` (an attached shell
   session owns every key except its prefix chord). Almost all input handling
   branches on `app.selected_block`.

4. **Event pipeline.** A raw `KeyEvent` is first checked for the attached
   shell view; while a shell is attached, `key_event/shell/view.rs` handles the
   prefix chord and forwards every other key to the PTY as terminal bytes.
   `Event::Mouse` and `Event::Paste` are routed to the same view when a shell is
   attached and the child has enabled the corresponding mode. In the
   browser/command line, `handle_event` first handles special keys (`Esc`,
   `Enter`, arrows, `Tab`, `Backspace`, control chords), then routes ordinary
   characters to the active `SwitchCase` menu, to the command line, or through
   the keymap into an `AppCommand`, which is executed by `AppCommand::execute`.

5. **Menus via `SwitchCase`.** Every "operation page" (paste, delete, tab,
   goto, macro, shell, …) is a temporary modal implemented with `SwitchCase`: it
   replaces the command line with a help message and routes the next character
   to a handler function pointer. Extra state is passed through
   `SwitchCaseData` (`None` / `Char` / `Bool` / `Struct`).

6. **Asynchronous work is channel-based.** Incremental file search and image
   decoding/resizing run on worker threads and communicate with the main loop
   through `std::sync::mpsc` channels.

7. **Shell sessions are PTY-backed.** Each mounted shell owns a PTY, a reader
   thread and a `vt100` screen. The reader thread parses the PTY output into the
   shared screen (behind a `Mutex`) as it arrives, so a background shell keeps
   running and keeps its screen up to date even while hire is blocked in a
   foreground process. Only shell *exits* travel through a channel. The
   `vt100::Parser`/`Screen` is rendered into the ratatui buffer by
   `ui/shell_block.rs`; keyboard events are translated back into terminal bytes
   by `key_event/shell/input.rs`. The attached child's protocol state also
   drives the *outer* terminal: `key_event/shell/mouse.rs` re-encodes mouse
   events for the child's tracking mode/encoding, and `sync_terminal_modes`
   enables/disables outer mouse capture and bracketed paste to match the child
   (so the browser never captures the mouse).

---

## 2. Directory layout

```
hire/
├── Cargo.toml            # package manifest, dependencies, release profile
├── Cargo.lock
├── README.md             # user-facing documentation
├── LICENSE
├── install.sh            # build + install binary/keymap/scripts
├── keymap_config/        # shipped keymap presets (default.toml, colemak.toml)
├── scripts/              # helper scripts (hire-rg.sh for the rg/fzf jump)
├── docs/                 # project documentation (this file, SHELL_SESSIONS.md)
└── src/
    ├── main.rs           # binary entry point & event loop
    ├── app/              # application state and domain model
    ├── command/          # command abstraction and filesystem commands
    ├── config/           # user/auto config and keymap loading
    ├── error/            # error types and macros
    ├── key_event/        # input handling and every interactive operation
    ├── ui/               # ratatui widgets, layout and rendering
    └── utils/            # shared low-level types and helpers
```

---

## 3. `src/` file-by-file reference

### `src/main.rs` — entry point

Declares all top-level modules, parses CLI arguments (`utils::Args`), builds the
default `App`, initializes config, starts the image/search background channels,
enters the ratatui alternate screen, and runs the main loop.

Responsibilities:

- `main()` — the event loop: run pending macro, reap shells, answer/forward the
  shells' graphics (`App::handle_shell_graphics`), draw UI, poll/read key
  events, handle the `q` quit shortcut, dispatch keys, drain the search and
  image channels.
- `shell_in_workdir` — `--working-directory` mode: jump to the stored working
  directory and spawn a shell immediately.
- `check_output` / `check_start_path` — apply `--output-file` /
  `--quit-after-output` / `--start-from`.
- `macro_check_before_key` / `macro_check_with_key` — replay a recorded macro
  (`execute_macro`) before reading input, and record keys while a macro is being
  recorded.
- `check_quit_condition` / `really_quit` — confirmation page when other tabs are
  open.

### `src/app/` — application state and domain model

| File | Purpose |
| --- | --- |
| `app/mod.rs` | Defines the central `App<'a>` struct and its core implementation. Re-exports `Macro`/`MacroStatus`/`MacroTarget`, `TermColors`, `FileSaver`, `sort`. |
| `app/filesaver.rs` | The `FileSaver` per-entry metadata struct and its rendering helpers; `sort()`. |
| `app/color.rs` | `TermColors`: maps `LS_COLORS` entries to ratatui styles; `from_ls_colors` falls back to `app/default_ls_colors.txt` when the variable is missing or blank. |
| `app/image_preview.rs` | `ImagePreview`: terminal image support detection, decode/resize worker threads, channels, `get_image_info`; `protocol_type` / `font_size` expose the detected protocol and cell size (the shell graphics support is derived from them). |
| `app/macro_utils.rs` | Macro state: `MacroStatus`, `MacroTarget`, `Macro`. |

**`app/mod.rs`** is the heart of the data model:

- `App<'a>` — path, `hide_files`, `quit_now`; the three file lists
  (`parent_files`, `current_files`, `child_files`) and their `ItemIndex`
  selection states; `file_content`; the active `selected_block`; marked files;
  command-line/command-history state; completion state; the file searcher;
  macro attributes; `TermColors`; bookmarks (`target_dir`); path history
  (`path_history`); working directory (`working_directory`); tabs; image
  preview; edit mode; keymap; loaded config; error queue; output settings;
  host/user names.
- Shell-session state: `shell_sessions` (`ShellSessions`), `shell_prefix`
  (`ShellPrefix`), `shell_prefix_active`, `shell_scroll_mode` (the viewport is
  scrolled back, so the scroll bindings own the keys and the mouse),
  `shell_pending_kill` (one shell),
  `shell_pending_kill_others` (the shell which stays, every other one of its tab
  is killed) and `shell_pending_kill_all` (a tab id; every shell of that tab),
  `shell_recent` (the most recently attached shells, newest first — `B`'s
  fallback when the current tab has none) and `shell_return_tab` (the tab to go
  back to after detaching a borrowed shell),
  `shell_digits` and `shell_digit_deadline` (numbered selection), `clear_screen`
  (full repaint after attaching/detaching) and `shell_notice` (an error shown in
  the shell status line instead of replacing the view).
- Outer-terminal state for the shell view: `outer_mouse_mode` and
  `outer_bracketed_paste` (what hire has enabled on the real terminal),
  `mouse_buttons` (buttons currently held, used to end a drag cleanly) and
  `shell_area` (the rectangle of the shell screen, used to translate/clamp
  mouse coordinates).
- File-list lifecycle: `init_all_files`, `init_parent_files`,
  `init_current_files`, `init_child_files`, `read_files`, `refresh_parent_item`,
  `refresh_current_item`, `refresh_child_item`, `refresh_select_item`,
  `select_normal`, `update_with_prev_selected`, `partly_update_block`.
- Navigation & lookup: `goto_dir`, `root`, `current_path`, `search_file`,
  `get_file_saver`, `get_file_saver_mut`, `get_directory_mut`, `hide_or_show`.
- Preview: `set_file_content`.
- Marked files: `append_marked_file(s)`, `marked_file_contains`,
  `marked_file_contains_path`, `remove_marked_file`, `clear_path_marked_files`.
- Free helpers: `path_is_hidden`, `filesave_closure`, `list_state_select`,
  `get_host_info`.

**`app/filesaver.rs`** — `FileSaver` stores `name`, `owner`, `is_file`, `is_dir`,
`executable`, `cannot_read`, `dangling_symlink`, `symlink_file` plus private
`size`, `permissions` and `modified_time`. It exposes `permission_span`,
`owner_span`, `modified_span`, `size_span`, `symlink_span`, `read_only`,
`set_modified`.
`sort()` orders directories before files, each group alphabetically.

**`app/color.rs`** — parses the `LS_COLORS` environment variable into
`dir_style`, `file_style`, `marked_style`, `orphan_style`, `symlink_style` and
`executable_style`. `TermColors::init` is infallible: when `LS_COLORS` is
missing (CI, a bare `sh`, a minimal container) or blank, `from_ls_colors` uses
the palette bundled in `src/app/default_ls_colors.txt` (the author's interactive
zsh `LS_COLORS`, a standard `dircolors` value). An empty value is treated like
an unset one, so the styles are still readable instead of all-default.

**`app/image_preview.rs`** — wraps `ratatui-image`'s `Picker` and
`ThreadProtocol`; spawns a decode thread and a resize/encode thread and returns
receivers to the main loop. `ImagePreview::useless` tells the loop whether there
is anything to render. The picker is queried once at startup (`CSI 16t` gives the
cell size used both for the preview and for the shells' kitty pixel size);
`Picker` is not `Copy`, so the accessors borrow it.

The preview itself is drawn by `ui/child_block.rs`: `StatefulImage` (the
replacement of the old `ThreadImage`) renders into `block.inner(layout)`, and
`pin_image_cells` then marks every cell whose symbol contains an escape
sequence with `CellDiffOption::ForcedWidth(1)`. This is required on ratatui
0.30: `Buffer::diff` derives a cell's width from its symbol, and a kitty/iTerm2/
sixel row symbol embeds the whole image row, so without the forced width the
diff skips the rest of the screen (only part of the image is drawn and the
following cells/layout are not updated).

**`app/macro_utils.rs`** — `Macro` keeps the recorded `KeyEvent` sequence, an
optional list of `MacroTarget { dir, name }` (used to apply a macro to every
marked file) and the current `MacroStatus` (`Recording`, `Executing`, `None`).

### `src/command/` — command abstraction

| File | Purpose |
| --- | --- |
| `command/mod.rs` | Wires the submodules and re-exports `cmds::*` and `AppCommand`. |
| `command/types.rs` | `AppCommand` enum and `AppCommand::from_str` (keymap string → command). |
| `command/cmds.rs` | Filesystem commands: `rename_file`, `create_file`, `create_symlink`, `file_exists`. |
| `command/cmd_utils.rs` | Command-line mode plumbing on `App`/`Block` (set/append/quit/parse, history, expand). |

**`command/types.rs`** — `AppCommand` enumerates every user-triggerable action
(tab/goto/paste/delete/search/refresh, path history (`PrevPath` / `PathHistory`),
edit mode, navi index, shell commands, `shell_command` from the keymap, etc.).
`from_str` parses the `run = "…"` value of a keymap entry.

**`command/cmd_utils.rs`** — extends `App` and `Block`:

- `Block::set_command_line` — replace the command-line content/cursor.
- `App::command_line_append`, `command_select` (history navigation),
  `quit_command_mode`.
- `App::command_parse` — parse an entered `:command` line. Handles `/` search,
  `:rename`, `:create_file`, `:create_dir`, `:create_symlink`, `:!<shell>`
  commands, dispatching to `command/cmds.rs` and `key_event/shell`.
- `expand_init` / `expand_quit` / `expand_scroll` — the expanded command-line
  view (used to display menus and long messages).

**`command/cmds.rs`** — the actual filesystem side effects for `:rename`,
`:create_file` / `:create_dir` (`create_file` with `is_dir`), and
`:create_symlink`, including refreshing the affected file lists.

### `src/config/` — configuration

| File | Purpose |
| --- | --- |
| `config/mod.rs` | Locate/read/write `auto_config.toml` and `user_config.toml`, `init_config`, and the `option_get!` macro. |
| `config/types.rs` | `Config` / `ConfigValue` / `AppConfig`: typed user settings and defaults. |
| `config/keymap.rs` | `Keymap` (normal/edit/navi maps plus the shell scroll bindings) and `keymap.toml` loading. |

**`config/mod.rs`** — config lives in `~/.config/springhan/hire/` (override it
with the `HIRE_CONFIG_DIR` environment variable, which keeps tests and
automated runs away from the user's real configuration; see §6 for the test
recipe). The hidden `--config-dir` argument sets an override even higher, which
is how a sudo session runs with its throw-away config copy. Three files are used:
`auto_config.toml` (written by the app: bookmarks, stored tabs),
`user_config.toml` (hand-written: `default_shell`, `gui_commands`,
`file_read_program`, `gui_file_open_cmd`, `shell_prefix`) and `keymap.toml`.
`get_document`/`write_document` wrap `toml_edit`. The `option_get!` macro
unwraps an `Option` into an early error return.

**Dev config filenames.** `get_conf_file` contains a deliberate developer
toggle: the block selecting `auto_config_dev.toml` / `user_config_dev.toml` /
`keymap_dev.toml` can be enabled in the working tree to test a new feature
without polluting the real configuration. When those names are active, hire
reads and writes only the `*_dev.toml` files (in the normal config directory or
under `HIRE_CONFIG_DIR`); the production files are untouched. This is an
expected local working-tree state, **not** a startup error and not something to
revert automatically: the committed/default block uses the production names,
while a developer may temporarily flip it. Scripts, unit tests and pty
harnesses which prepare a config directory must check which block is active and
create the matching file names; see §6, "Dev config filenames".

**`config/types.rs`** — `Config` holds a name plus a `ConfigValue`
(`String`/`Vec`) and can be built from defaults and populated from a TOML
`Item`. `AppConfig` is the collection stored on `App`.

**`config/keymap.rs`** — `Keymap` keeps three `HashMap<char, AppCommand>`
(normal, edit, navi index) and two ordered `Vec<(KeySpec, ScrollCommand)>` for the
attached shell view. `init_keymap` reads `keymap.toml`; `insert_keybinding`
routes each command into the correct map(s) (e.g. navi-input keys, edit-mode-only
keys, commands available everywhere). The optional `shell_scroll` and
`shell_scroll_mode` arrays put a small DSL of scroll commands (`scroll up`,
`scroll page down`, `scroll top`, `scroll quit`, …) on full key specs
(`M-u`, `C-b`, `PageUp`, …); they start from built-in defaults (`M-u`/`M-e`,
`k`/`j`/`PgUp`/`PgDn`/`g`/`G`/`q`/…), and a file entry replaces the binding of
the same key so a single key can be remapped. `shell_scroll_label` exposes the
first binding of each command for the status-line hint.

### `src/error/` — errors

| File | Purpose |
| --- | --- |
| `error/mod.rs` | Re-exports the error types and defines the `rt_error!` macro. |
| `error/types.rs` | `AppResult`, `AppError`, `ErrorType`, `NotFoundType` and conversions. |

`AppError` collects multiple `ErrorType`s so a batch operation can report every
failure at once. `ErrorType` covers permission denied, invalid command, no
selection, not-found, I/O, environment and `anyhow` errors. `rt_error!`
early-returns an `anyhow`-backed error.

### `src/key_event/` — input handling and operations

| File | Purpose |
| --- | --- |
| `key_event/mod.rs` | Module root, `handle_event`, `AppCommand::execute`, public re-exports. |
| `key_event/command_line.rs` | Command-line editing, completion popup and cursor movement. |
| `key_event/cursor_movement/mod.rs` | File-list navigation, scrolling, marking while moving, navi-index jump. |
| `key_event/cursor_movement/types.rs` | `Goto` and `NaviIndex`. |
| `key_event/edit/mod.rs` | Edit-mode behaviour (batch create/rename/delete) and applying changes. |
| `key_event/edit/types.rs` | `EditItem` and `EditMode`. |
| `key_event/file_operations.rs` | Rename prefill, delete menu/confirmation, marking, physical delete. |
| `key_event/file_search.rs` | Incremental search worker, match navigation. |
| `key_event/goto_operation.rs` | Bookmark ("goto") menu; persists bookmarks to `auto_config.toml`. |
| `key_event/interaction.rs` | External tools: `fzf_jump`, `rg_jump`, `vim_diff`. |
| `key_event/macro_page.rs` | Macro menu: record/execute, and execute-per-marked-file. |
| `key_event/path_history.rs` | Path history menu: `goto_prev_path`, `path_history_operation` / `path_history_switch`. |
| `key_event/paste_operation.rs` | Paste menu (move/copy/symlink), paste engine, origin removal. |
| `key_event/shell/mod.rs` | Module wiring; `cmdline_shell` (prefill `:!<shell>`) and re-exports for the session API. |
| `key_event/shell/types.rs` | `CommandStr`, `ShellCommand` (one-shot foreground commands) and `$.` substitution. |
| `key_event/shell/utils.rs` | `default_shell`, run one-shot commands (with or without output; `execute_command` clears the screen afterwards, because the command shares hire's terminal), open files, working-dir cache. |
| `key_event/shell/graphics.rs` | `GraphicsSupport`, `GraphicsFilter`: splits kitty graphics APC sequences out of the shell output, answers the child's terminal queries (device attributes, cell/text-area size, status, and always the `CSI 6n` cursor report) and forwards placeholder image transmissions to the real terminal. `TextScreen` is the contract with the parser; the text is fed query by query so a report reflects the cursor position at the query. |
| `key_event/shell/emulation.rs` | `Hvp`: the escape sequences `vt100` does not implement, translated before they reach the parser. Today `CSI f` (HVP) becomes `CSI H` (CUP); programs which cannot read the terminal size otherwise move the cursor to the bottom right with HVP and ask for its position (`gh`'s prompts do). The state survives across reads, so a sequence split between two reads is still translated. |
| `key_event/shell/pty.rs` | `PtyProcess`: spawn in a PTY (with the pixel size derived from the cell size), reader thread parsing into the shared `vt100` screen through the graphics filter (which also applies the emulation fixes), write, resize, kill + reap, and `foreground_pid` (the terminal's current foreground process group). |
| `key_event/shell/session.rs` | `ShellMeta`, `ShellSession`, `ShellSessions`: the per-tab shell registry. `display_title` names the program running in the session (its foreground process), falling back to the shell program; `drain_graphics` hands the reader threads' graphics events to the main loop. |
| `key_event/shell/input.rs` | `KeySpec` (key + modifiers parsed from `C-s`/`M-u`/`PageUp`/…), `ShellPrefix` (built on `KeySpec`), `encode_key` (`KeyEvent` → terminal bytes) and `encode_paste`. |
| `key_event/shell/mouse.rs` | `encode_mouse`: `MouseEvent` → the child's mouse protocol mode/encoding (SGR, X10/Default, UTF-8), wheel included (only `Shift`+wheel and the scroll mode keep it for hire). |
| `key_event/shell/view.rs` | Attach/detach/open/switch one shell (`attach` switches to the shell's tab when it belongs to another one and remembers `shell_return_tab`; `attach_active` handles `attach_shell`, including the most-recent-shell fallback), the attached-view key state machine (`C-s x` kill, `C-s D` kill every shell of the tab, `C-s o` kill every other shell and keep the attached one, `C-s [` scroll mode), exit cleanup, numbered selection, `App::close_tab_shells` (the one place which clears a tab's shells) and `App::close_other_shells` (keep one shell of a tab), `graphics_support` and `App::handle_shell_graphics`; `detach` re-reads the current directory (`refresh_after_shell`) and, for a borrowed shell, refreshes the shell's tab and returns to `shell_return_tab`. Mouse handling forwards the wheel to the child by default (only `Shift`+wheel or the scroll mode scrolls hire's history), and `outer_mouse_mode` keeps the terminal's native selection when no one needs the mouse. |
| `key_event/shell/scroll.rs` | The shell scrollback: `ScrollCommand` (parsed from `keymap.toml`), `enter`/`leave`/`apply`, `handle_wheel`; it only moves `vt100`'s scrollback offset and keeps `shell_scroll_mode` in sync with it (any history above the viewport keeps the mode, the bottom leaves it). |
| `key_event/shell/menu.rs` | The shell menu (`shell_menu`): lists the current tab's shells first and then the shells of the other tabs (`menu_entries`); digits attach any of them (a shell of another tab is borrowed), `n` creates on the current tab, `x` + digits and `D` only touch the current tab; `D` asks before clearing every shell of the tab. |
| `key_event/simple_operations.rs` | Print full path, write output file / run GUI open command, jump to stored path. |
| `key_event/sudo_session.rs` | Root session: tab hand-over, isolated config, `sudo -E` launch and tab restore. |
| `key_event/switch/mod.rs` | The `SwitchCase` menu framework. |
| `key_event/switch/traits.rs` | `SwitchStruct` / `SwitchClone` traits, `Clone` for `SwitchCaseData`. |
| `key_event/tab/mod.rs` | Tab module wiring and restoring stored tab groups. |
| `key_event/tab/types.rs` | `TabId`, `Tab` (path, hidden flag, selection, mounted shells) and `TabList`; `TabState` (tab menu state) and `TabAction` (the destructive operation waiting for confirmation). |
| `key_event/tab/utils.rs` | Tab operations: open/close/switch/save/restore; every removal (`c`, `d` + number, `o`, restoring a stored group) asks first when mounted shells would be killed. `switch_to_tab` saves the current tab's view state and restores the target's; `next`/`prev`/`switch_tab` and the shell borrow use it. |

**`key_event/mod.rs`** — the input dispatcher.

- `handle_event` dispatches everything to `key_event/shell/view.rs` while
  `selected_block` is `Block::Shell`. Otherwise it handles modifier chords
  (`Ctrl-b/f/a/e/n/p`, `Ctrl-g`, `Ctrl-Backspace`, `Alt-f` / `Alt-b`),
  `Backspace`, `Esc`, `Enter`, arrows, and `Tab` (completion / `Alt-Tab`
  expand). Non-control characters go to the active `SwitchCase`, the command
  line, or the keymap.
- `impl AppCommand::execute` is the single match that turns every `AppCommand`
  into a concrete call into the operation modules. `AppCommand::Shell` calls
  `shell::open_shell` (a mounted, attachable session); `AppCommand::ShellMenu`
  opens the shell menu; `AppCommand::ShellAttach` calls
  `shell::attach_active`, which attaches the shell remembered by the current
  tab (`Tab.active_shell`) or, when the tab has none, the most recently
  attached shell of another tab (`App.shell_recent`).
- Re-exports the public types used by `app`, `ui` and `main`
  (`TabList`, `FileSearcher`, `EditMode`, `SwitchCase`, `ShellCommand`, …).

**`key_event/command_line.rs`** — `AppCompletion` (candidate list, popup
position, selected candidate); `completion` (TAB completion: builtin commands
for `:xxx`, files for arguments, and completing the selected browser item when
the cursor follows a space); `switch_to` (move through candidates);
`update_cmdline` (write a candidate back); `get_content` (read the text up to
the cursor); `App::cursor_left` / `App::cursor_right`, the word-wise
`cursor_word_forward` (`Alt-f`) / `cursor_word_backward` (`Alt-b`),
`delete_word_before_cursor` (`Ctrl-Backspace`) / `delete_word_after_cursor`
(`Alt-d`) and `delete_char_after_cursor` (`Ctrl-d`), all of which are shared
with edit mode through the helpers in `utils`. The cursor is a UTF-8 byte
offset on a character boundary, so movement and deletion always jump over whole
characters; the completion popup is positioned and sized by the display width
(`utils::display_width`) instead of the byte length.

**`key_event/cursor_movement/`** — `directory_movement` (left/right into
parent/child, right opens files), `move_cursor` (up/down/scroll/index, also
expands the mark range while `mark_expand` is on), `move_cursor_core`, and
`jump_to_index` for the navigation index.

**`key_event/edit/`** — an inline batch editor over the current directory.
`EditMode` holds one `EditItem` per file plus newly created items, supports
insert mode, cursor movement (including word-wise `Alt-f` / `Alt-b`, the
`Ctrl-Backspace` / `Alt-d` whole-word deletions and `Ctrl-d` character
deletion, applied to every item whose cursor is active), marking and delete
flags. Cursors are byte offsets on character boundaries, so multi-byte names
are edited whole; `save_edit` applies all renames/creations/deletions at once
behind a confirmation page.

**`key_event/file_operations.rs`** — `append_file_name` (prefill `:rename` for
the selected file, cursor before the extension), `delete_operation` /
`delete_switch` (delete menu and confirmation), `delete_file` (physical removal
plus selection fix-up), `mark_operation` (mark/unmark one or all files) and the
menu message builders.

**`key_event/file_search.rs`** — `FileSearcher` keeps the matched indices and a
sender to a matching worker thread. `init_search_channel` starts that thread,
`file_search` (async) / `file_search_sync` request matches, and
`next_candidate` / `prev_candidate` move to the next/previous match.

**`key_event/goto_operation.rs`** — the bookmark menu. `goto_switch` handles
jumping to a stored directory and entering `+`/`-` sub-modes to add/remove a
bookmark by key; changes are persisted through `config` and read back by
`read_config`.

**`key_event/interaction.rs`** — launches external tools: `fzf_jump` (fuzzy
find, then jump to the chosen file), `rg_jump` (run the bundled `hire-rg.sh`)
and `vim_diff` (diff the selected and marked files).

**`key_event/macro_page.rs`** — `macro_operation` opens the macro page,
`macro_switch` starts recording on `q` and requests execution on `e`
(collecting marked files as targets when any exist), `execute_macro` replays the
recorded keys once — or once per target after selecting it —
`collect_marked_targets` / `select_target` implement that per-file loop, and
`generate_msg` builds the page text.

**`key_event/path_history.rs`** — keeps the paths the user jumped from in
`App::path_history` (at most 30, newest last, duplicates moved to the end).
`goto_prev_path` jumps back to the last stored path and pushes the path it left;
`path_history_operation` / `path_history_switch` show those paths numbered `01`
to `30` and jump to the one whose number the user types.

**`key_event/paste_operation.rs`** — `paste_operation` / `paste_switch` present
the paste menu (move, copy, force copy/move, symlink, clear marks);
`paste_files` performs the copy (including recursive directory copy and
overwrite handling); `make_single_symlink` builds a `:create_symlink` command;
`remove_origin_files` deletes sources after a move.

**`key_event/shell/`** — two kinds of shell work.

*One-shot foreground programs.* `ShellCommand` now only has the
`Command(shell, args)` variant; `shell_process` temporarily leaves the alternate
screen, runs the program on the real terminal and restores the TUI (optionally
refreshing the file list). It is used by `:!`, `shell_command`,
`open_file_in_shell` and `vim_diff`. `fetch_output` captures stdout;
`execute_command` appends the target path and clears the screen once the
command exits, since the command runs on hire's own terminal (in raw mode and
inside the alternate screen) and whatever it printed would stay on the browser
otherwise. The clear also resets the render buffer, so the next frame repaints
the whole UI. The working-directory cache
(`fetch_working_directory`, `set_working_directory`, `load_working_directory`)
serves `--working-directory` and the state-line indicator.

*Mounted sessions.* `view.rs`'s `open_shell` spawns `default_shell` into the
`ShellSessions` registry (`session.rs`), pushes the id onto
`Tab.shells` and attaches it. `attach`/`detach` flip `Block::Shell` and
`Tab.active_shell`; the session keeps running while detached. An attached shell
always belongs to the current tab: attaching one which belongs to another tab
(a digit in the shell menu, or `B`'s fallback) switches to that tab through
`tab::switch_to_tab` and stores the tab the user came from in
`App.shell_return_tab`; `detach` refreshes the shell's tab and then switches
back, so borrowing a background shell does not leave the user on a different
tab. From the browser, `attach_shell` (`B`) calls `attach_active`, which
attaches the shell the tab used last (or the most recent shell of another tab
when this tab has none); it is the quick counterpart of the shell menu and
needs no numbering. `handle_view_key`
implements the attached-view state machine: the prefix chord (`C-s`, from
`shell_prefix`), detach/switch/create/kill, and the numbered selection
(0-based; with more than ten shells the digits are accumulated and committed on
`Enter`, when no longer ambiguous, or after a 450 ms timeout). `handle_shell_events`
reaps exited sessions and attaches the next shell of the tab (or returns to the
browser), `tick_shell_digits` performs the timeout commit, and
`remove_shell_from_tab` keeps the tab's shell list consistent.

`pty.rs`'s `PtyProcess` owns the PTY master and the reader thread; the thread
parses every chunk into the shared `vt100::Parser` behind a `Mutex`, so a
background shell keeps consuming output even while hire runs a foreground
program. Only exits travel through a channel. `foreground_pid` calls
`tcgetpgrp` on the master (it reports the slave's foreground process group), and
`process_name` reads `/proc/<pid>/comm`, so the menu and the status line show
`vim` while vim owns the shell and the shell's own name at the prompt.
`input.rs` translates crossterm
`KeyEvent`s back into terminal bytes, following the child's DECCKM state for
the arrow keys and Home/End (`ESC O A` versus `ESC [ A`), and encodes pasted
text as a bracketed paste when the child asked for one. `mouse.rs` translates
`event::MouseEvent` into the child's mouse protocol mode (`Press` /
`PressRelease` / `ButtonMotion` / `AnyMotion`) and encoding (`Sgr` / `Default` /
`Utf8`), dropping events the child's mode does not want and translating the
coordinates from the outer terminal into the shell area. `sync_terminal_modes`
is called every loop: it captures the mouse (SGR-encoded) only while the child
wants it and only while the view is attached, and enables bracketed paste on
the outer terminal when the child does; `disable_terminal_modes` restores both
before hire exits. `menu.rs` is the `T` page listing the
tab's shells, each labelled with the program running in it. The screen is
rendered by `ui/shell_block.rs`.

**`key_event/simple_operations.rs`** — `print_full_path` (show the selected
file's full path in a read-only page), `output_path` (run the configured
`gui_file_open_cmd` with the selected path or the current directory appended
when it's set, otherwise write that path to the temp/output file; both honor
`quit_after_output`; it hands the terminal to `execute_command` so the UI is
cleared after an external command) and `jump_to_temp_file` (jump to the path stored in that
file; `normalize_target` strips trailing separators so a `/home/` entry becomes
`/home` before `goto_dir`).

**`key_event/sudo_session.rs`** — `SessionTabs` (the focused tab index plus every
tab's path, `hide_files` flag and selected file index) is serialized to a
temporary TOML file; `sudo_session` copies the config files into a temporary
directory, leaves the TUI and runs
`sudo -E <current_exe> --sudo-session <state> --config-dir <dir>` in the same
terminal. The new process reads `--sudo-session` at startup
(`App::apply_session_tabs`) and runs with the temporary config
(`--config-dir`). Quitting the root instance returns to the original,
unprivileged one — there is no euid juggling in either process. Mounted shell
sessions are **not** transferred (they live in the parent process); they keep
running while the root instance is open and are still mounted when it returns.

**`key_event/switch/`** — the generic menu framework. `SwitchCase::new` installs
a function pointer plus `SwitchCaseData` and shows a `CmdContent::Text` message;
`switch_match` invokes the handler with the pressed character and closes the
page when the handler returns `true`. `traits.rs` lets arbitrary structs
(`TabState`) travel through `SwitchCaseData::Struct` as `Box<dyn SwitchStruct>`.

**`key_event/tab/`** — multi-tab support. `TabList` stores one `Tab` per open
tab: stable `TabId`, path, hidden-file flag, remembered selection, the mounted
`shells: Vec<ShellId>` and `active_shell`. `tab_operation` opens the tab page;
`switch` handles create/next/prev/close/close-others and numeric selection
(1-based; multi-digit when there are more than nine tabs). Every removal (`c`,
`d` + number, `o`, and applying a stored group) checks the affected tabs first: a mounted shell turns
the action into a `SwitchCase` confirmation (`TabAction` in `TabState`, drawn
like the other y/N pages, `expand: false`), and only `y` runs it — any other key
returns to the tab menu. Confirmed removals call `App::close_tab_shells`, which
detaches first when the attached shell belongs to a cleared tab.
`save_tabs` / `apply_storage_tabs` / `remove_storage_tabs` manage named tab
groups persisted in `auto_config.toml`; applying a group asks first because
sessions cannot be persisted and the shells of the replaced tabs are killed.
`read_config` restores the groups. `SessionTabs` (sudo) snapshots only path,
hidden flag and selection, never shells.

### `src/ui/` — rendering

| File | Purpose |
| --- | --- |
| `ui/mod.rs` | Top-level layout and draw entry point `ui`, error banner, title bar. |
| `ui/parent_block.rs` | Renders the parent-directory column. |
| `ui/current_block.rs` | Renders the current-directory column (or the edit-mode list). |
| `ui/child_block.rs` | Renders the child column and the file/text/image preview; image cells get `CellDiffOption::ForcedWidth(1)` so ratatui's diff does not treat their escape-laden symbols as hundreds of columns wide. |
| `ui/command_line.rs` | Renders the state line / inline command line and status labels. |
| `ui/cmdline_popup.rs` | Renders the completion candidate popup. |
| `ui/list.rs` | Custom list widget with sidebars, navigation index and selection. |
| `ui/shell_block.rs` | Renders the attached shell's `vt100` screen (`screen.cell` follows the scrollback offset) and its status line; the right-hand key hint is only shown while the prefix is active, the scroll mode is open (built from the configured bindings) or a prompt/notice needs it. The child's cursor is only drawn on the live screen. |
| `ui/utils.rs` | Builds list items and their styles from files / edit items. |

**`ui/mod.rs`** — computes the vertical layout (title, browser, command line;
or title + expanded command line), renders the title bar
(`user@host`, shortened path, selected file name) and the item statistics,
calls `check_app_error` (turns `app.app_error` into a red command-line message),
then lays out and draws the parent/current/child blocks, the command line and
the completion popup. `title_bar_path` canonicalises the browser path to exactly
one trailing separator, so a path which still ends with `/` does not render as
`/home//name`. When `selected_block` is `Block::Shell`, it short-circuits
to `ui/shell_block.rs`: the title bar keeps `SH n/total`, the middle band is the
shell screen and the bottom band is the shell status line. The title's tab badge
shows the mounted shell count (`|1:2|`).

**`ui/list.rs`** — the reusable `List` / `Item` widgets. `Item` renders a left
line plus an optional right sidebar; `List` handles the visible window, selection
highlight, offset adjustment, the "Empty" state and the navigation-index
digits. `prefix_split` highlights the typed prefix of the index number.

**`ui/utils.rs`** — `render_list` converts `FileSaver`s into styled items
(directory/file/executable/symlink/orphan colours plus the marked sidebar);
`render_editing_list` does the same for `EditItem`s, including the edit cursor
(highlighted with the same byte-offset logic as the command line) and
delete/marked sidebars; `shorten_path` abbreviates a path so it fits in the
state line.

**`ui/shell_block.rs`** — `render_shell` resizes the parser and the PTY to the
drawn area and copies every `vt100::Cell` into the ratatui buffer (fg/bg
colours, bold/italic/underline/inverse; wide-cell continuations are skipped).
A blank cell is written as a real space: `vt100::Cell::contents()` is empty for
cleared cells and an empty symbol would not erase the old character on the
outer terminal. `screen.cell()` follows the scrollback offset set by
`key_event/shell/scroll.rs`, so the same loop renders the history; the child's
cursor is only placed with `Frame::set_cursor_position` while the offset is 0.
`shell_status_lines` builds the shell's left/right status text (the program
running in the shell — `vim` while vim is open, the shell's own name at its
prompt — its start directory, position, prefix hint, the scroll-mode hint
`SCROLL n` with the configured key labels, the pending
single-shell/kill-all/kill-others prompt, typed shell number, or
`shell_notice`).

**`ui/command_line.rs`** — `render_command_line` draws either the browser state
line (permission/owner/size/link info on the left and, on the right, the
working directory path when set plus the status labels
`WD EXPAND -- RECORDING -- QUIT`) or the editable command line; the
`Block::Shell` arm reuses `StateLine` for `shell_status_lines`;
`state_labels` builds the status label row; `get_command_line_span_list`
produces the character spans with the block cursor highlighted, comparing the
cursor byte offset with `char_indices()` so multi-byte characters are
highlighted correctly; `StateLine` is the small two-part widget used for the
state line.

**`ui/cmdline_popup.rs`** — `CompletionPopup` and `render_completion` position
and draw the completion candidate list above the command line. The position and
width come from `utils::display_width`, so CJK candidates line up with the
command line; the layout arithmetic saturates instead of overflowing.

### `src/utils/` — shared helpers

| File | Purpose |
| --- | --- |
| `utils/mod.rs` | CLI `Args`, `Direction`, text helpers, window-height global. |
| `utils/types.rs` | Core shared enums/structs used across modules. |

**`utils/mod.rs`**

- `Args` — clap-derived CLI options (`--working-directory`, `--start-from`,
  `--output-file`, `--quit-after-output`), plus the hidden `--sudo-session` /
  `--config-dir` arguments used by the sudo session.
- `Direction` — Up/Down/Left/Right parsed from keymap strings.
- `str_split`, `read_to_text` (read a limited number of lines, strip `\r`,
  expand tabs, parse ANSI), `delete_word` (character-based, so multi-byte
  content is never split).
- Word-wise text editing shared by the command line and edit mode:
  `is_word_char`, `next_word_end` / `prev_word_start`,
  `cursor_next_word_end` / `cursor_prev_word_start`,
  `delete_word_before_cursor` / `delete_word_after_cursor` and
  `delete_char_after_cursor`.
- `CursorPos` handling: `cursor_insertion_point` / `cursor_from_insertion_point`
  (byte offset on a character boundary) and `prev_char_boundary` /
  `next_char_boundary` for character-wise movement and deletion.
- `display_width` — the terminal column width of a text (wide characters take
  two columns), used by the command line cursor and the completion popup.
- The global rendered window height (`WINDOW_HEIGHT`, `update_window_height`,
  `get_window_height`) used to size scrolling and preview reads.

**`utils/types.rs`**

- `CursorPos` (`Index` / `End` / `None`) — text cursor representation.
  `Index` is a UTF-8 byte offset which must lie on a character boundary,
  `End` means after the last character and `None` means no editable cursor.
- `CmdContent` (`String` / `Text`) — editable command-line content vs.
  styled, read-only messages.
- `ShellId(u64)` — the opaque identity of a shell session, also carried by
  `Block::Shell`.
- `Block` (`Browser(bool)` / `CommandLine(CmdContent, CursorPos)` /
  `Shell(ShellId)`) — which block currently owns input. `Block::shell_id()` and
  `Block::is_shell()` are the small helpers used by the view and the UI.
- `MarkedFiles` — marked names per directory.
- `ItemIndex` — the three `ListState`s (parent/current/child) plus accessors.
- `SearchFile` — which list a lookup refers to.
- `FileContent` — `None` / `Text` / `Image`, the preview payload.

---

## 4. Main flows

### Key dispatch

```
crossterm Event
  ├─ Key(KeyEvent)
  │    └─ main.rs: quit shortcut (skipped in the shell view) / macro recording
  │         └─ key_event::handle_event
  │              ├─ Block::Shell?       → shell::handle_view_key
  │              │                         ├─ prefix chord? → detach/switch/create/kill/number
  │              │                         └─ otherwise → encode_key → write to the PTY
  │              ├─ modifier chords (Ctrl-…)
  │              ├─ special keys: Backspace, Esc, Enter, ↑/↓/←/→, Tab
  │              ├─ active SwitchCase?  → switch_match → handler
  │              ├─ Block::Browser?     → keymap lookup → AppCommand → execute
  │              └─ Block::CommandLine? → append character to the command line
  ├─ Mouse(MouseEvent) (capture is only enabled for a mouse-aware shell)
  │    └─ shell::handle_view_mouse → encode_mouse → write to the PTY
  └─ Paste(String) (bracketed paste is only enabled for a shell)
       └─ shell::handle_view_paste → encode_paste → write to the PTY
```

### File browser model

Three columns (`parent_files`, `current_files`, `child_files`) with matching
`ListState`s in `ItemIndex`. Selection is always tracked per column; `self.path`
is the current directory, except in the root directory where the "current"
column is derived from the selected parent entry. `FileSaver`s carry the
metadata shown in the state line and preview.

### Command line and completion

`:`/`/`/`!` switch `selected_block` to `CommandLine`. `command_parse`
(`command/cmd_utils.rs`) interprets the line. `Tab` triggers
`key_event::command_line::completion`, which completes builtin commands, file
names, or the selected browser item (when the cursor is right after a space) and
shows a candidate popup (`ui/cmdline_popup.rs`).

### Shell sessions

```
spawn_shell (`S`) / --working-directory
  └─ shell::open_shell
       ├─ ShellSessions::open → PtyProcess::spawn
       │    ├─ openpty + spawn `default_shell` (cwd = app.path)
       │    ├─ reader thread: PTY bytes → Arc<Mutex<vt100::Parser>>
       │    └─ exit channel
       ├─ Tab.shells.push(id); Tab.active_shell = Some(id)
       └─ attach → Block::Shell(id), clear_screen
            └─ main loop: 16 ms poll, handle_view_key forwards keys
                 ├─ `C-s d` → detach: Block::Browser + refresh the current
                 │    directory (and, for a borrowed shell, go back to the
                 │    tab the shell was borrowed from), session still mounted
                 ├─ `C-s n/p`, `C-s <number>` → attach another shell
                 ├─ `C-s c` → open another shell on the same tab
                 ├─ `C-s x` → prompt; `y` kills it and attaches the next
                 ├─ `C-s D` → prompt; `y` kills every shell of the tab
                 │    (App::close_tab_shells) and returns to the browser
                 ├─ `C-s o` → prompt; `y` kills the tab's other shells and
                 │    keeps the attached one (App::close_other_shells)
                 ├─ `C-s [` → scroll mode: the `shell_scroll_mode` bindings
                 │    (k/j, PgUp/PgDn, g/G, q/Esc…) move vt100's scrollback
                 │    offset; scrolling back to the bottom leaves the mode
                 ├─ `M-u` / `M-e` (shell_scroll) → scroll without the mode;
                 │    scrolling up enters it
                 ├─ wheel events → forwarded to the child (vim/less keep
                 │    their own mouse scrolling) unless `Shift` is held or
                 │    the scroll mode is open: then they scroll hire's history
                 ├─ mouse events → encode_mouse (child's mode/encoding) → PTY;
                 │    ignored while the scroll mode is open
                 ├─ paste events → encode_paste (bracketed if the child asked) → PTY
                 ├─ sync_terminal_modes: outer mouse capture + bracketed paste
                 │    are enabled only while this shell asks for them; the
                 │    scroll mode asks for press reporting so its wheel works
                 └─ shell exits → handle_shell_events reaps it, removes the id
                      from the tab and attaches the next live shell; when there
                      is none it detaches, which also refreshes the browser
                      (and returns to the tab a borrowed shell came from)

shell_menu (`T`)
  ├─ list the current tab's shells first (0-based), then the other tabs'
  │    shells under `Tab N`; digits attach either kind, `n` new, `x` + digits
  │    kill (current tab only)
  ├─ attaching a shell of another tab → attach switches that tab, stores
  │    `shell_return_tab`; `C-s d` refreshes that tab and switches back
  └─ `D` → prompt; `y` kills every shell of the current tab and keeps the tab
       open (App::close_tab_shells); the menu is shown again

attach_shell (`B`)
  └─ shell::attach_active
       ├─ attach(Tab.active_shell) — the shell this tab attached last,
       │    however it was chosen (`S`, the menu, `C-s n/p`, `C-s <number>`)
       └─ no shell on this tab → attach the most recently attached shell of
            another tab (`App.shell_recent`), with the same borrow-and-return
            behaviour; an error only when no tab has a shell
```

`ui/shell_block.rs` renders the attached screen into the middle band, and
`render_command_line`'s `Block::Shell` arm renders its status line. Quitting
hire confirms when mounted shells are alive; `PtyProcess::drop` kills and reaps
them.

### Images inside a shell

```
child writes to its PTY → GraphicsFilter (reader thread)
  ├─ text                        → vt100 screen (as before)
  ├─ terminal queries            → GraphicsEvent::Reply → the child's PTY
  │    ├─ kitty `a=q`            → `OK` (direct) / `ENOTSUP` (shared memory, files)
  │    ├─ `CSI c` (DA1)          → `?62;22c` (no sixel, so the child picks kitty)
  │    ├─ `CSI 14t` / `CSI 16t`  → text area / cell size in pixels
  │    └─ `CSI 6n`               → `CSI <row>;<col>R`, from the child's screen
  │                                 (answered even without kitty support)
  └─ kitty image data            → GraphicsEvent::Forward → the real terminal
       (`a=T`, `U=1`, chunked)      (one buffered sequence per image, `q=2`)
                                      ↓
            the child's unicode placeholder cells, which `ui/shell_block.rs`
            renders like any other cell, make kitty draw the image there
```

`GraphicsSupport` is `kitty` only when hire's own picker detected the kitty
protocol for the real terminal; otherwise the child receives no graphics answers
and behaves exactly as before. Inside tmux the support is off as well, because
the sequences would need tmux passthrough wrapping. `a=d` deletes are translated
to deletes of the image ids this shell forwarded, so a TUI inside a shell can
never remove hire's own preview image.

The cursor report is not a graphics answer: a real terminal always sends it, and
a program which asked for it waits for the report before it reads anything else
(`gh`'s text prompts block exactly there, and swallow the typed keys while they
look for the `R`). It is therefore answered with `GraphicsSupport::disabled()`
too, with the position of the child's own `vt100` screen and one-based like
every terminal reports it. The bytes are parsed up to the query before the
answer is computed, so a `CSI 6n` followed by more cursor movement still reports
the position at the query.

Because `vt100` ignores `CSI f` (HVP), a program which probes the screen size
that way would get the wrong position, and its whole screen would be painted in
the wrong place. `emulation::Hvp` rewrites the final `f` to `H` before the
parser sees it; the rewrite is one byte for one byte, so the filter's offsets
and its split-query detection are unaffected.

### Tab removal

```
tab menu (`t`)
  ├─ `c`            → remove the current tab
  ├─ `d` + number   → remove that tab
  ├─ `o`            → remove every other tab
  ├─ `s` + number   → replace every open tab with a stored group
  └─ mounted shells on the affected tabs?
       ├─ no  → run it at once
       └─ yes → confirmation page ("... will kill N shells ...")
            ├─ `y` → run it (`close_tab_shells`, then remove/replace)
            └─ any other key → back to the tab menu, nothing changed
```

Every removal goes through `App::close_tab_shells`, so it detaches first when
the attached shell belongs to a removed tab, kills its sessions and clears
`Tab.shells` / `Tab.active_shell` before the tab (or the whole list) is
dropped. Restoring a stored group asks too, because it replaces every open tab.
Clearing a tab's shells without removing the tab lives in the **shell** menu
(`T`, then `D`), not in the tab menu.

### Sudo session

```
sudo_session (keybinding, e.g. `Z`)
  └─ sync the current tab → SessionTabs → /tmp/hire-sudo-*/session.toml
     copy auto_config/user_config/keymap → the same temporary directory
       └─ sudo -E <hire> --sudo-session <file> --config-dir <dir>
            └─ new hire: config override + apply_session_tabs + TUI
                 └─ quit (`q`, confirm with `y` when tabs are open)
                      └─ the root process exits, the original TUI is redrawn
```

### Menus (`SwitchCase`)

A menu is created with `SwitchCase::new(app, handler, expand, message, data)`.
It sets the command-line block to the (usually multi-line) help text and stores
the handler. Subsequent characters are delivered to the handler until it returns
`true`, at which point `quit_command_mode` restores the browser. Complex menu
state (e.g. tab operations) is boxed in `SwitchCaseData::Struct`.

### Background work

- **Search:** `FileSearcher` sends `(query, exact, files)` to a worker thread
  started by `init_search_channel`; the main loop polls the receiver and updates
  the match list.
- **Images:** two threads decode and resize/encode images; the main loop polls
  their receivers and updates `file_content` / the image protocol.

### Macros

`main.rs` records keys while `MacroStatus::Recording`, and before each iteration
replays a recorded macro when `MacroStatus::Executing` by calling
`key_event::execute_macro`. The macro page
(`key_event/macro_page.rs`) starts recording (`q`) or requests execution (`e`);
when files are marked, execution is repeated for every marked target.

---

## 5. Files outside `src/`

| Path | Purpose |
| --- | --- |
| `Cargo.toml` | Package metadata, dependencies and the size-optimized release profile (`opt-level = 's'`, LTO, stripped). |
| `keymap_config/default.toml`, `keymap_config/colemak.toml` | Shipped keymap presets; `install.sh` copies the selected one to `~/.config/springhan/hire/keymap.toml`. |
| `scripts/hire-rg.sh` | `rg` + `fzf` helper invoked by the `rg_search` command. |
| `install.sh` | Builds the release binary and installs the binary, keymap and scripts. |
| `README.md` | User documentation (configuration, features, keybindings). |
| `docs/ARCHITECTURE.md` | This document. |
| `docs/SHELL_SESSIONS.md` | Design record for the implemented mountable (tmux-like) shell sessions: PTY/session model, key handling, numbered selection and edge cases. |
| `rust-toolchain.toml` | Pins the stable channel and the `rustfmt`/`clippy` components for local development and CI. |
| `.github/workflows/ci.yml` | CI: build and test on `ubuntu-latest` with `--locked`. |
| `.github/dependabot.yml` | Weekly Cargo and GitHub Actions update PRs. |

---

## 6. Running tests

`cargo test` only runs the unit tests. They never launch the application and
never execute the user configuration: the only config-related test
(`goto_test::test_read_config`) builds its TOML document in memory. The tests
which spawn processes run `/bin/sh` in a private PTY (no config file, no hire
binary) and return early when `/bin/sh` does not exist; they need no network and
no installed binary. The suite is also independent of `LS_COLORS`:
`TermColors::init` falls back to the bundled palette when the variable is unset
or blank, and `app::color::test::*` covers the fallback and the parsing of an
explicit value, so `cargo test` works in a bare CI shell. They are
`key_event::shell::session::test_display_title_follows_the_foreground_program`
(the foreground/title lookup),
`key_event::shell::session::test_cursor_position_query_is_answered` (a
`printf '\033[5;9H\033[6n'` run in a real `/bin/sh` session gets `CSI 5;9R`
back through the reader thread), `key_event::shell::graphics::test::*` (the
kitty queries and the cursor report, including a report split between two reads
and one which must ignore the bytes after the query),
`key_event::shell::emulation::test::*` (`CSI f` → `CSI H`, split sequences and
plain text/OSC/DCS payloads left alone), `key_event::shell::view::test_*` (`C-s D`
kill-all, `C-s o` kill-others, detach refresh, `attach_active` and its
cross-tab fallback), `key_event::shell::scroll::test::*`
(scrolling, paging, top/bottom, wheel, leaving at the bottom),
`key_event::shell::input::test::*` (`KeySpec` parsing/labels, `shell_prefix`),
`config::keymap::test::*` (the default scroll bindings and the `keymap.toml`
sections), `key_event::shell::menu::test::*` (the shell menu's `D`
confirmation, the cross-tab list/borrow and the `x` scope),
`ui::shell_block::test::*` (the idle status line and the prefix
hint) and `key_event::tab::utils::test::*` (tab-removal confirmations).
`key_event::simple_operations::test::*` covers the temp-file target
normalisation (`/home/` → `/home`) and `ui::test::*` the title's single
trailing separator.

**Any test that drives the real binary — a pty harness, an expect-like script or
a manual smoke test — must isolate the config with `HIRE_CONFIG_DIR`.** Otherwise
the app reads the developer's `~/.config/springhan/hire/`, and pressing `Enter`
in the browser calls `output_path`, which runs the configured
`gui_file_open_cmd` (e.g. `emacsclient -n`) and opens windows on the developer's
desktop.

The isolated directory needs the keymap file which the active config block
selects (the app expects one) and an empty user-config file (so
`gui_file_open_cmd` keeps its empty default; the app creates the
`auto_config*` file itself). A minimal setup:

```bash
tmp=$(mktemp -d)
cp keymap_config/default.toml "$tmp/keymap.toml"
: > "$tmp/user_config.toml"

HIRE_CONFIG_DIR="$tmp" cargo test
HIRE_CONFIG_DIR="$tmp" ./target/debug/hire --start-from "$workdir" --output-file "$tmp/out"
rm -rf "$tmp"
```

With the isolated config, `Enter` in the browser writes the path into the output
file (`--output-file`, or `/tmp/hire.tmp` when it isn't given) instead of
starting a GUI program, so an e2e harness can check the result safely.

**CI.** `.github/workflows/ci.yml` runs on `ubuntu-latest` with the stable
toolchain pinned by `rust-toolchain.toml`: `cargo build --locked --all-targets`
and `cargo test --locked`. The suite is Linux-oriented (a real PTY and
`/bin/sh`; the foreground-process tests read `/proc`), so there is no Windows
job, and a macOS job would need those tests gated first. `clippy` and `rustfmt`
are installed for the future lint jobs but are not enforced yet: the repository
still has pre-existing lint/format debt, and those checks are enabled after
their cleanup PR. Dependabot keeps `Cargo.lock` and the workflow actions
current.

### Dev config filenames

`get_conf_file` has a deliberate developer toggle (see §3). Before preparing a
config directory, check which `format!` block is active in
`src/config/mod.rs::get_conf_file`:

| Block | Files |
| --- | --- |
| production (committed/default) | `keymap.toml`, `user_config.toml`, `auto_config.toml` |
| dev (local working-tree toggle) | `keymap_dev.toml`, `user_config_dev.toml`, `auto_config_dev.toml` |

The app creates a missing `auto_config*` file itself, but it panics in
`init_keymap` if the keymap file is missing, so a harness must create the name
matching the active block. When the dev block is active:

```bash
tmp=$(mktemp -d)
cp keymap_config/default.toml "$tmp/keymap_dev.toml"
: > "$tmp/user_config_dev.toml"
HIRE_CONFIG_DIR="$tmp" ./target/debug/hire ...
```

Do not rename the files or flip the block just to satisfy a harness; use the
names the current source reads. A working tree with the dev block active is a
normal development state for testing a feature without polluting the real
configuration — it is not a bug and must not be reverted automatically (the
committed state uses the production names).

A pty harness has one more trap: at startup `ratatui-image`'s
`Picker::from_query_stdio` writes a capability query and waits one second for
`ESC[6;<h>;<w>t` (cell size) and `ESC[0n` (status). A bare pty never answers, and
the query's reader thread keeps reading stdin afterwards, eating whatever keys
the harness sends. Reply with e.g. `\x1b[6;20;10t\x1b[0n` before typing. A real
terminal answers immediately, so interactive use is unaffected.

`ratatui`'s `Terminal::clear` (which every attach schedules through
`app.clear_screen`) asks the real terminal for its cursor position with
`ESC[6n` and blocks for up to two seconds when nothing answers; reply with e.g.
`\x1b[10;20R`. Without that reply the app exits with "The cursor position could
not be read within a normal duration" — the same message a shell session's
child no longer sees, because hire answers the child's own `ESC[6n` now.

Mouse and paste can be exercised the same way: once the child program enables a
mouse mode (e.g. `printf '\033[?1000h\033[?1006h'`), write an SGR mouse report
such as `\x1b[<0;10;10M` to the harness; once it enables bracketed paste
(`\033[?2004h`), write `\x1b[200~text\x1b[201~`. If the child reads raw bytes,
it sees the sequence hire forwarded.

For shell sessions, a pty harness also needs a real window size: set
`TIOCSWINSZ` on the pty (e.g. 40x120). A 0x0 pty makes `WINDOW_HEIGHT` zero and
the list renderer underflows.

The shipped keymaps don't bind `:`; use the configured `create_file`,
`create_dir` or `search` key (e.g. `=` in `keymap_config/default.toml`) to reach
the command line from the browser.
