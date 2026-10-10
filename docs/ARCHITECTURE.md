# Hire — Architecture

`hire` (ligHtweight fIle bRowsEr) is a terminal file manager written in Rust, in the
spirit of `ranger`. It is a single-binary, keyboard-driven TUI application.

- **Language / edition:** Rust 2024 (`Cargo.toml`, package `hire`).
- **TUI stack:** [`ratatui`](https://crates.io/crates/ratatui) 0.29 with
  `crossterm` as the backend, plus `ratatui-image` for inline image preview.
- **Supporting crates:** `clap` (CLI args), `toml_edit` (config files),
  `thiserror` + `anyhow` (errors), `chrono` (timestamps), `file-size`,
  `copy_dir`, `is_executable`, `image`, `ansi-to-tui`, `lazy_static`.

---

## 1. High-level design

Hire is an event-driven application built around one mutable state object, `App`.

### Prcoess
1. CLI args → App::default() → init_config()
2. loop:
   a. execute pending macro
   b. ui::ui(frame, &mut app)
   c. poll/read terminal key event
   d. handle_event(key, &mut app, terminal)
   e. drain search / image channels

### Modules

- `main.rs` — Entrance, start the main loop
- `key_event::handle_event` — Handle key input
  - Input：key event
  - Outupt：AppCommand
  - Function call：AppCommand::execute
- `app::App` — Global state Container
  - Block::Browser | Block::CommandLine
- `ui::ui` — Only read state of App and render ui
  - Render areas：parent / current / child / file / cmdline

### Data stream

key event → key_event::handle_event → AppCommand → AppCommand::execute → App
                                                                          ↑
ui::ui Read state of App ←───────────────────────────────────────────────┘


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

3. **Two block modes.** `utils::Block` is either `Browser(bool)` (the file
   browser; the bool means "current directory is `/`") or
   `CommandLine(CmdContent, CursorPos)`. Almost all input handling branches on
   `app.selected_block`.

4. **Key pipeline.** A raw `KeyEvent` is first handled for special keys
   (`Esc`, `Enter`, arrows, `Tab`, `Backspace`, control chords) inside
   `handle_event`. Ordinary characters are either routed to the active
   `SwitchCase` menu, to the command line, or looked up in the keymap and turned
   into an `AppCommand`, which is then executed by `AppCommand::execute`.

5. **Menus via `SwitchCase`.** Every "operation page" (paste, delete, tab,
   goto, macro, …) is a temporary modal implemented with `SwitchCase`: it
   replaces the command line with a help message and routes the next character
   to a handler function pointer. Extra state is passed through
   `SwitchCaseData` (`None` / `Char` / `Bool` / `Struct`).

6. **Asynchronous work is channel-based.** Incremental file search and image
   decoding/resizing run on worker threads and communicate with the main loop
   through `std::sync::mpsc` channels.

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
├── docs/                 # project documentation (this file)
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

- `main()` — the event loop: run pending macro, draw UI, poll/read key events,
  handle the `q` quit shortcut, dispatch keys, drain the search and image
  channels.
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
| `app/color.rs` | `TermColors`: maps `LS_COLORS` entries to ratatui styles. |
| `app/image_preview.rs` | `ImagePreview`: terminal image support detection, decode/resize worker threads, channels, `get_image_info`. |
| `app/macro_utils.rs` | Macro state: `MacroStatus`, `MacroTarget`, `Macro`. |

**`app/mod.rs`** is the heart of the data model:

- `App<'a>` — path, `hide_files`, `quit_now`; the three file lists
  (`parent_files`, `current_files`, `child_files`) and their `ItemIndex`
  selection states; `file_content`; the active `selected_block`; marked files;
  command-line/command-history state; completion state; the file searcher;
  macro attributes; `TermColors`; bookmarks (`target_dir`); path history
  (`path_history`); tabs; image preview; edit mode; keymap; loaded config;
  error queue; output settings; host/user names.
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

**`app/filesaver.rs`** — `FileSaver` stores `name`, `is_file`, `is_dir`,
`executable`, `cannot_read`, `dangling_symlink`, `symlink_file` plus private
`size`, `permissions` and `modified_time`. It exposes `permission_span`,
`modified_span`, `size_span`, `symlink_span`, `read_only`, `set_modified`.
`sort()` orders directories before files, each group alphabetically.

**`app/color.rs`** — parses the `LS_COLORS` environment variable into
`dir_style`, `file_style`, `marked_style`, `orphan_style`, `symlink_style` and
`executable_style`.

**`app/image_preview.rs`** — wraps `ratatui-image`'s `Picker` and
`ThreadProtocol`; spawns a decode thread and a resize/encode thread and returns
receivers to the main loop. `ImagePreview::useless` tells the loop whether there
is anything to render.

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
| `config/keymap.rs` | `Keymap` (normal/edit/navi maps) and `keymap.toml` loading. |

**`config/mod.rs`** — config lives in `~/.config/springhan/hire/`. Three files
are used: `auto_config.toml` (written by the app: bookmarks, stored tabs),
`user_config.toml` (hand-written: `default_shell`, `gui_commands`,
`file_read_program`) and `keymap.toml`. `get_document`/`write_document` wrap
`toml_edit`. The `option_get!` macro unwraps an `Option` into an early error
return.

**`config/types.rs`** — `Config` holds a name plus a `ConfigValue`
(`String`/`Vec`) and can be built from defaults and populated from a TOML
`Item`. `AppConfig` is the collection stored on `App`.

**`config/keymap.rs`** — `Keymap` keeps three `HashMap<char, AppCommand>`
(normal, edit, navi index). `init_keymap` reads `keymap.toml`; `insert_keybinding`
routes each command into the correct map(s) (e.g. navi-input keys, edit-mode-only
keys, commands available everywhere).

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
| `key_event/shell/mod.rs` | `cmdline_shell`: prefill the command line with `:!<shell>`. |
| `key_event/shell/types.rs` | `CommandStr`, `ShellCommand` and `$.` substitution. |
| `key_event/shell/utils.rs` | Run shells/commands, capture output, open files, working-dir cache. |
| `key_event/simple_operations.rs` | Print full path, write output file, jump to stored path. |
| `key_event/switch/mod.rs` | The `SwitchCase` menu framework. |
| `key_event/switch/traits.rs` | `SwitchStruct` / `SwitchClone` traits, `Clone` for `SwitchCaseData`. |
| `key_event/tab/mod.rs` | Tab module wiring and restoring stored tab groups. |
| `key_event/tab/types.rs` | `TabList` (open tabs) and `TabState` (tab menu state). |
| `key_event/tab/utils.rs` | Tab operations: open/close/switch/save/restore. |

**`key_event/mod.rs`** — the input dispatcher.

- `handle_event` handles modifier chords (`Ctrl-b/f/a/e/n/p`, `Ctrl-g`),
  `Backspace`, `Esc`, `Enter`, arrows, and `Tab` (completion / `Alt-Tab`
  expand). Non-control characters go to the active `SwitchCase`, the command
  line, or the keymap.
- `impl AppCommand::execute` is the single match that turns every `AppCommand`
  into a concrete call into the operation modules.
- Re-exports the public types used by `app`, `ui` and `main`
  (`TabList`, `FileSearcher`, `EditMode`, `SwitchCase`, `ShellCommand`, …).

**`key_event/command_line.rs`** — `AppCompletion` (candidate list, popup
position, selected candidate); `completion` (TAB completion: builtin commands
for `:xxx`, files for arguments, and completing the selected browser item when
the cursor follows a space); `switch_to` (move through candidates);
`update_cmdline` (write a candidate back); `get_content` (read the text up to
the cursor); `App::cursor_left` / `App::cursor_right`.

**`key_event/cursor_movement/`** — `directory_movement` (left/right into
parent/child, right opens files), `move_cursor` (up/down/scroll/index, also
expands the mark range while `mark_expand` is on), `move_cursor_core`, and
`jump_to_index` for the navigation index.

**`key_event/edit/`** — an inline batch editor over the current directory.
`EditMode` holds one `EditItem` per file plus newly created items, supports
insert mode, cursor movement, marking and delete flags; `save_edit` applies all
renames/creations/deletions at once behind a confirmation page.

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

**`key_event/shell/`** — `types.rs` defines `CommandStr` (`Str` /
`SelectedItem`, where `$.` expands to the selected file) and `ShellCommand`
(`Shell` or `Command(shell, args)`). `utils.rs` implements `shell_process`
(temporarily leaves the alternate screen, runs the process, restores the TUI,
optionally refreshes the file list), `fetch_output` (capture stdout),
`open_file_in_shell` (image viewer or configured file reader), and the
working-directory cache used by `--working-directory`.

**`key_event/simple_operations.rs`** — `print_full_path` (show the selected
file's full path in a read-only page), `output_path` (write the selected path or
the current directory to the temp/output file, honoring `quit_after_output`) and
`jump_to_temp_file` (jump to the path stored in that file).

**`key_event/switch/`** — the generic menu framework. `SwitchCase::new` installs
a function pointer plus `SwitchCaseData` and shows a `CmdContent::Text` message;
`switch_match` invokes the handler with the pressed character and closes the
page when the handler returns `true`. `traits.rs` lets arbitrary structs
(`TabState`) travel through `SwitchCaseData::Struct` as `Box<dyn SwitchStruct>`.

**`key_event/tab/`** — multi-tab support. `TabList` stores each tab's path,
hidden-file flag and remembered selection. `tab_operation` opens the tab page;
`switch` handles create/next/prev/close/close-others and numeric selection;
`save_tabs` / `apply_storage_tabs` / `remove_storage_tabs` manage named tab
groups persisted in `auto_config.toml`; `read_config` restores them.

### `src/ui/` — rendering

| File | Purpose |
| --- | --- |
| `ui/mod.rs` | Top-level layout and draw entry point `ui`, error banner, title bar. |
| `ui/parent_block.rs` | Renders the parent-directory column. |
| `ui/current_block.rs` | Renders the current-directory column (or the edit-mode list). |
| `ui/child_block.rs` | Renders the child column and the file/text/image preview. |
| `ui/command_line.rs` | Renders the state line / inline command line and status labels. |
| `ui/cmdline_popup.rs` | Renders the completion candidate popup. |
| `ui/list.rs` | Custom list widget with sidebars, navigation index and selection. |
| `ui/utils.rs` | Builds list items and their styles from files / edit items. |

**`ui/mod.rs`** — computes the vertical layout (title, browser, command line;
or title + expanded command line), renders the title bar
(`user@host`, shortened path, selected file name) and the item statistics,
calls `check_app_error` (turns `app.app_error` into a red command-line message),
then lays out and draws the parent/current/child blocks, the command line and
the completion popup.

**`ui/list.rs`** — the reusable `List` / `Item` widgets. `Item` renders a left
line plus an optional right sidebar; `List` handles the visible window, selection
highlight, offset adjustment, the "Empty" state and the navigation-index
digits. `prefix_split` highlights the typed prefix of the index number.

**`ui/utils.rs`** — `render_list` converts `FileSaver`s into styled items
(directory/file/executable/symlink/orphan colours plus the marked sidebar);
`render_editing_list` does the same for `EditItem`s, including the edit cursor
and delete/marked sidebars.

**`ui/command_line.rs`** — `render_command_line` draws either the browser state
line (permission/size/link info on the left and the status labels
`EXPAND -- RECORDING -- QUIT` on the right) or the editable command line;
`state_labels` builds the status label row; `get_command_line_span_list`
produces the character spans with the block cursor highlighted; `StateLine` is
the small two-part widget used for the state line.

**`ui/cmdline_popup.rs`** — `CompletionPopup` and `render_completion` position
and draw the completion candidate list above the command line.

### `src/utils/` — shared helpers

| File | Purpose |
| --- | --- |
| `utils/mod.rs` | CLI `Args`, `Direction`, text helpers, window-height global. |
| `utils/types.rs` | Core shared enums/structs used across modules. |

**`utils/mod.rs`**

- `Args` — clap-derived CLI options (`--working-directory`, `--start-from`,
  `--output-file`, `--quit-after-output`).
- `Direction` — Up/Down/Left/Right parsed from keymap strings.
- `str_split`, `read_to_text` (read a limited number of lines, strip `\r`,
  expand tabs, parse ANSI), `delete_word`.
- The global rendered window height (`WINDOW_HEIGHT`, `update_window_height`,
  `get_window_height`) used to size scrolling and preview reads.

**`utils/types.rs`**

- `CursorPos` (`Index` / `End` / `None`) — text cursor representation.
- `CmdContent` (`String` / `Text`) — editable command-line content vs.
  styled, read-only messages.
- `Block` (`Browser(bool)` / `CommandLine(CmdContent, CursorPos)`) — which
  block currently owns input.
- `MarkedFiles` — marked names per directory.
- `ItemIndex` — the three `ListState`s (parent/current/child) plus accessors.
- `SearchFile` — which list a lookup refers to.
- `FileContent` — `None` / `Text` / `Image`, the preview payload.

---

## 4. Main flows

### Key dispatch

```
crossterm KeyEvent
  └─ main.rs: quit shortcut / macro recording
       └─ key_event::handle_event
            ├─ modifier chords (Ctrl-…)
            ├─ special keys: Backspace, Esc, Enter, ↑/↓/←/→, Tab
            ├─ active SwitchCase?  → switch_match → handler
            ├─ Block::Browser?     → keymap lookup → AppCommand → execute
            └─ Block::CommandLine? → append character to the command line
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
