# Mountable Shell Sessions (tmux-like)

**Status:** implemented. This document is the design record for the feature;
the code below matches the current tree. The only parts still unimplemented
are marked in §17 (phase 3).

---

## 1. Goal

`spawn_shell` (`AppCommand::Shell`) currently runs a shell in the foreground:
`shell_process` leaves the alternate screen, disables raw mode and blocks in
`wait()` (`src/key_event/shell/utils.rs`). Hire owns nothing while the shell
runs, so the shell cannot be moved to the background, and a tab can only ever
have the one shell that is currently running.

Target behaviour:

- `spawn_shell` starts a **shell session** owned by hire and shows it in the
  current tab (the *shell view*).
- One key chord detaches the session: the file browser comes back, the shell
  keeps running in the background, and it stays **mounted on the current tab**.
- A tab owns an ordered list of shells; at most one is attached (visible) at a
  time. A shell menu lists the tab's shells and attaches / creates / kills them.
- `spawn_shell` again creates a *new* shell on the same tab, so a tab can hold
  several background shells, like tmux windows.
- Closing a tab kills its shells; quitting hire kills every mounted shell.

Non-goals (explicitly out of scope for this plan):

- Sessions surviving a hire restart (PTYs die with the process).
- Split panes / several shells visible at once.
- Transferring shells to the sudo session.
- Persisting shells in stored tab groups (`storage_tabs`).

---

## 2. Why the existing shell cannot simply be detached

`shell_process` hands the real terminal to the child:

```rust
disable_raw_mode()?;
execute!(stdout(), LeaveAlternateScreen, Show)?;
process.spawn()?.wait()?;          // hire is blocked here
enable_raw_mode()?;
execute!(stdout(), EnterAlternateScreen, Hide)?;
```

Three consequences:

1. Hire reads no key while the shell runs, so there is no key that could mean
   "detach": the bytes go straight to the child.
2. The child has no terminal of its own. If hire took the screen back, the
   child would keep writing escape sequences to the same terminal.
3. Nothing records what the shell drew, so a later re-attach could not restore
   the screen.

A detachable shell therefore needs, per session:

- a **PTY** (the child gets a private terminal; hire keeps the real one),
- a **reader thread** pumping the PTY master,
- a **VT parser / screen buffer** to replay the output into ratatui,
- a **key encoder** turning crossterm events back into terminal bytes,
- a **prefix key** so one chord can be intercepted while every other key is
  forwarded to the child.

---

## 3. Dependencies

```toml
portable-pty = "0.9"

# 0.16 is the first vt100 line whose scrollback works for offsets larger than
# one screen; it needs unicode-width 0.2.1+, hence ratatui 0.30 below.
vt100 = "0.16"

ratatui = "0.30"
ratatui-image = { version = "10", default-features = false, features = ["crossterm"] }
ansi-to-tui = "8"
```

- `portable-pty` — PTY creation, spawning, resize (`ioctl(TIOCSWINSZ)`, which
  also sends `SIGWINCH`), child kill/wait. Cross-platform, but this project is
  Unix-only anyway.
- `vt100` — byte-stream → screen (`Cell`s with contents, fg/bg colour, bold,
  italic, underline, inverse, wide-cell flags). It also models the alternate
  screen, so full-screen programs (vim, htop, lazygit) work.

`vt100 0.16.2` uses plain `u16` sizes but moved `set_size`/`set_scrollback`
onto `Screen` (hence `parser.screen_mut()`), and `Cell::contents` returns
`&str`:

```rust
let mut parser = vt100::Parser::new(rows.max(1), cols.max(1), SCROLLBACK_ROWS);
parser.screen_mut().set_size(rows.max(1), cols.max(1));
parser.process(&bytes);
parser.screen_mut().set_scrollback(offset);      // scrollback, 0 = live
parser.screen().cell(row, col);                  // follows the offset
```

`ratatui` 0.30 is required because 0.29 pins `unicode-width = "=0.2.0"` while
`vt100` 0.16 needs 0.2.1+; `ratatui-image` 10 is the line which supports
ratatui 0.30 (built without its default `chafa` feature), and `ansi-to-tui` 8
is the matching release.

`vt100::Parser` is `Send` (no `Rc`/`RefCell` inside), so the reader thread can
own a clone of it behind a `Mutex`.

---

## 4. Data model

```
App
 ├─ tab_list: TabList
 │    └─ Tab {
 │         id: TabId,                 // stable across removals
 │         path: PathBuf,
 │         hide_files: bool,
 │         selected: Option<usize>,
 │         shells: Vec<ShellId>,      // ordered, like tmux windows
 │         active_shell: Option<ShellId>,
 │       }
 ├─ shell_sessions: ShellSessions
 │    ├─ sessions: HashMap<ShellId, ShellSession>
 │    ├─ next_id: u64
 │    ├─ exit_tx / exit_rx: Sender/Receiver<ShellId>
 │    ├─ graphics_tx / graphics_rx: Sender/Receiver<(ShellId, GraphicsEvent)>
 │    └─ ShellSession {
 │         id: ShellId,
 │         tab: TabId,
 │         meta: ShellMeta { title, cwd, started_at },
 │         screen: Arc<Mutex<vt100::Parser>>,
 │         pty: PtyProcess,
 │         alive: bool,
 │       }
 ├─ shell_prefix: ShellPrefix,       // parsed from `shell_prefix` config
 ├─ shell_prefix_active: bool,       // next key is a prefix command
 ├─ shell_pending_kill: Option<ShellId>,
 └─ selected_block: Block::Browser(bool)
                  | Block::CommandLine(CmdContent, CursorPos)
                  | Block::Shell(ShellId)      // NEW
```

Why this shape:

- `ShellId` is an opaque, `Copy` id (`utils/types.rs`) so `Block::Shell` can
  carry it without creating a `utils -> key_event` dependency cycle.
- `TabId` is stable. `TabList` is positional today; shells must keep their
  owner when tabs before them are removed, so an id is needed.
- The screen is shared with the reader thread, not streamed through a channel:
  a background shell keeps consuming and parsing output even while hire is
  blocked in another foreground process (`:!vim`, `shell_command lazygit`,
  `open_file_in_shell`). No unbounded channel, no backpressure on a build that
  keeps printing while the user is elsewhere.
- Only **exit** events and **graphics** events travel through a channel. Exits
  are small and rare; graphics events are the query answers the child waits for
  and the image data the main loop has to write to the real terminal (a shell
  cannot write to it from its reader thread).

---

## 5. Tab refactor (preparatory, no behaviour change)

`TabList` currently keeps three parallel vectors that must be kept in sync by
hand (`list: Vec<(PathBuf, bool)>`, `selected_file`, `current`). Adding two more
(`shells`, `active_shell`) would make four. Replace them with one `Tab` struct:

```rust
// src/key_event/tab/types.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TabId(pub u64);

#[derive(Clone)]
pub struct Tab {
    pub id: TabId,
    pub path: PathBuf,
    pub hide_files: bool,
    pub selected: Option<usize>,
    pub shells: Vec<ShellId>,
    pub active_shell: Option<ShellId>,
}

pub struct TabList<'a> {
    current: usize,
    list: Vec<Tab>,
    storage: Vec<Rc<[Cow<'a, str>]>>,
    next_id: u64,
}
```

Accessors to add (all mutation of `list` goes through them):

```rust
impl<'a> TabList<'a> {
    pub fn new(path: PathBuf) -> Self;
    pub fn current(&self) -> usize;
    pub fn len(&self) -> usize;
    pub fn tabs(&self) -> &[Tab];
    pub fn current_tab(&self) -> &Tab;
    pub fn current_tab_mut(&mut self) -> &mut Tab;
    pub fn tab_by_id_mut(&mut self, id: TabId) -> Option<&mut Tab>;
    pub fn push(&mut self, path: PathBuf, hide_files: bool) -> TabId;
    pub fn remove(&mut self, idx: usize) -> Option<Tab>;
    pub fn set_tabs(&mut self, tabs: Vec<Tab>); // clamps `current`, always non-empty
}
```

Call sites to migrate:

| Site | Change |
| --- | --- |
| `TabList::new` | one `Tab` with `id = 0` |
| `snapshot` | **delete**; `SessionTabs::from_app` reads `tabs()` |
| `set_snapshot` | **rename** to `set_tabs(Vec<Tab>)`; sudo restore builds fresh `Tab`s (empty shells) |
| `create` | `push(path, hide)`; new `TabId` |
| `remove_base` | clone the tab's `shells`, close them, then `remove(idx)` |
| `delete_other_tabs` | keep the current `Tab` (id and shells), close the others |
| `handle_tabs` / `select_new` | use `current_tab().path/hide_files/selected` |
| `apply_storage_tabs` | close **all** current shells, then `set_tabs(...)` |
| `update_current_tab` | write `path/hide_files/selected` into `current_tab_mut()` |
| `generate_msg` / `tab_string_list` | render path + `shells.len()` badge |
| `sudo_session.rs` | build `SessionTab`s from `tabs()`; build `Tab`s on restore |
| `ui/mod.rs` title | unchanged (`current()`, `len()`) |

`Tab` never goes into the session TOML: `sudo_session` can only transfer path,
hidden-file flag and selected index.

---

## 6. PTY process wrapper

New file `src/key_event/shell/pty.rs`:

```rust
pub struct PtyProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
}

impl PtyProcess {
    pub fn spawn(
        program: &str,
        cwd: &Path,
        size: PtySize,                 // rows/cols + pixel size from the cell size
        screen: Arc<Mutex<vt100::Parser>>,
        exited: Sender<ShellId>,
        graphics: PtyGraphics,         // GraphicsSupport + the events sender
        id: ShellId,
    ) -> AppResult<Self>;

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
    pub fn resize(&mut self, rows: u16, cols: u16) -> AppResult<()>;
    pub fn kill(&mut self);
    pub fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>>;
}
```

Implementation notes:

- `native_pty_system().openpty(size)`; the pixel size is
  `cols × cell_width` by `rows × cell_height`, so programs which read the window
  size in pixels (or ask for it with `CSI 16t`) size their images correctly.
- `CommandBuilder::new(program)`, `.cwd(cwd)`; the base environment is inherited,
  so `TERM`, `COLORTERM`, `SSH_*` etc. are already correct. Keep the current
  non-login shell behaviour.
- `slave.spawn_command(cmd)`, then `drop(slave)`. The master stays alive for the
  lifetime of the session.
- `master.try_clone_reader()` + `master.take_writer()`; the writer is used by the
  main thread for key forwarding.
- Reader thread:

```rust
let mut buf = [0u8; 8192];
let mut filter = GraphicsFilter::new(graphics.support);
loop {
    match reader.read(&mut buf) {
        Ok(0) | Err(_) => break,
        Ok(n) => {
            let mut parser = screen.lock().unwrap_or_else(|err| err.into_inner());
            let (rows, cols) = parser.screen().size();
            let mut text = |chunk: &[u8]| parser.process(chunk);
            let mut emit = |event| { let _ = graphics.events.send((id, event)); };
            // Kitty graphics APCs are split off; the text still goes to vt100.
            filter.feed(&buf[..n], rows, cols, &mut text, &mut emit);
        }
    }
}
let _ = exited.send(id);   // main loop reaps the child and cleans the tab
```

- Closing the master (dropping the session) sends `SIGHUP` to the session's
  process group, so children of the shell do not survive. `kill()` is still
  called explicitly, and `process_group_leader()` may be used with
  `libc::killpg(pgid, SIGHUP)` for a hard cleanup.
- Convert `portable_pty::Error` at the boundary (`anyhow::anyhow!("pty: {e}")` or
  `.map_err(anyhow::Error::from)`); no new `ErrorType` variant is needed.

---

## 7. Session registry

New file `src/key_event/shell/session.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
// ShellId lives in utils/types.rs

pub struct ShellMeta {
    pub title: String,          // "bash", basename of the shell program
    pub cwd: PathBuf,           // directory the shell was started in
}

pub struct ShellSession {
    pub tab: TabId,
    pub meta: ShellMeta,
    pub alive: bool,
    screen: Arc<Mutex<vt100::Parser>>,
    pty: PtyProcess,
}

pub struct ShellSessions {
    sessions: HashMap<ShellId, ShellSession>,
    next_id: u64,
    exit_tx: Sender<ShellId>,
    exit_rx: Receiver<ShellId>,
    graphics_tx: Sender<(ShellId, GraphicsEvent)>,
    graphics_rx: Receiver<(ShellId, GraphicsEvent)>,
}

impl ShellSession {
    pub fn parser(&self) -> MutexGuard<'_, vt100::Parser>;
    pub fn write(&mut self, bytes: &[u8]) -> AppResult<()>;
    pub fn resize(&mut self, rows: u16, cols: u16) -> AppResult<()>;
    pub fn foreground_title(&self) -> Option<String>; // e.g. "vim"
    pub fn display_title(&self) -> String;           // foreground, else meta.title
}

impl ShellSessions {
    pub fn new() -> Self;
    pub fn open(&mut self, tab: TabId, program: &str, cwd: &Path,
                graphics: GraphicsSupport) -> AppResult<ShellId>;
    pub fn get(&self, id: ShellId) -> Option<&ShellSession>;
    pub fn get_mut(&mut self, id: ShellId) -> Option<&mut ShellSession>;
    pub fn is_alive(&self, id: ShellId) -> bool;
    pub fn drain_exits(&mut self) -> Vec<ShellId>;
    pub fn drain_graphics(&mut self) -> Vec<(ShellId, GraphicsEvent)>;
    pub fn close(&mut self, id: ShellId);          // kill + reap + drop
    pub fn close_many(&mut self, ids: &[ShellId]);
    pub fn alive_count(&self) -> usize;
}
```

- `display_title` is what the shell menu and the status line show: the program
  which currently owns the session's terminal, not the shell it was started
  with. `foreground_title` asks the PTY for its foreground process group
  (`tcgetpgrp` on the master, which reports the slave's group) and reads
  `/proc/<pgid>/comm`; at a prompt that is the shell itself (`bash`, `zsh`),
  with vim running it is `vim`. It returns `None` on a dead session or when
  `/proc` is unreadable (non-Linux), and `display_title` then falls back to
  `meta.title`. `meta.title` itself always keeps the shell program.
- `open` creates the parser (`Parser::new(24, 80, SCROLLBACK_ROWS)`; the first
  draw resizes it), spawns the PTY with the pixel size derived from
  `graphics.cell`, registers the session under a fresh `ShellId` and returns
  it. Registration into `Tab.shells` is done by the caller
  (`App::open_shell`), because only `App` can touch `TabList`.
- `drain_exits` is called once per main-loop iteration; the caller removes the
  id from the owning tab, reaps the child with `close`, and fixes the view if
  the exited session was attached.
- `Drop for PtyProcess` kills and reaps the child, so dropping `ShellSessions`
  (which happens when `App` is dropped on quit) never leaves orphans.

---

## 8. Input: prefix key and key encoding

New file `src/key_event/shell/input.rs`.

### 8.1 Prefix

```rust
#[derive(Clone, Copy)]
pub struct ShellPrefix {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Default for ShellPrefix {
    fn default() -> Self { Self { code: KeyCode::Char('b'), modifiers: KeyModifiers::CONTROL } }
}

impl ShellPrefix {
    pub fn parse(spec: &str) -> anyhow::Result<Self>; // "C-s", "M-x", "C-M-x", "F12", "b"
    pub fn matches(&self, event: &KeyEvent) -> bool;
}
```

Default `C-s` (the user's choice, not tmux's `C-b`). It is configurable because
hire is often run *inside* tmux, whose own prefix would swallow `C-b`; `C-s`
also has to be remapped inside some terminals. `C-s` is normally XOFF, but hire
reads the real terminal in raw mode (IXON disabled) and intercepts the chord
before forwarding anything, so the shell never sees it.

### 8.2 Encoding

```rust
pub fn encode_key(event: &KeyEvent) -> Option<Vec<u8>>;
```

Cover the keys a terminal application can receive:

| crossterm | bytes |
| --- | --- |
| `Char(c)` (none / shift) | UTF-8 of `c` |
| `Char(c)` + `Ctrl` | `c & 0x1f` (`Ctrl-Space` = NUL, `Ctrl-[` = ESC, …) |
| `Char(c)` + `Alt` | `ESC` + UTF-8 of `c` |
| `Enter` / `Tab` | `\r` / `\t` |
| `BackTab` | `ESC [ Z` |
| `Backspace` | `0x7f` |
| `Esc` | `ESC` |
| Arrows | `ESC O A/B/C/D` when the child enabled application cursor mode (DECCKM), otherwise `ESC [ A/B/C/D` |
| `Home` / `End` | `ESC O H` / `ESC O F` in application mode, otherwise `ESC [ H` / `ESC [ F` |
| `PageUp` / `PageDown` | `ESC [ 5~` / `ESC [ 6~` |
| `Insert` / `Delete` | `ESC [ 2~` / `ESC [ 3~` |
| `F1..F4` / `F5..F12` | `ESC O P..S` / `ESC [ 15~,17~,18~,19~,20~,21~,23~,24~` |
| anything else | `None` (ignored) |

Only `KeyEventKind::Press` is forwarded (the main loop already drops releases).
Paste arrives as a burst of `Char` events (bracketed paste is not enabled), so
a per-frame drain loop (see §10) is enough to keep it responsive.

`encode_key` takes the child's DECCKM state as a second argument;
`write_key` reads it from `parser().screen().application_cursor()` before
locking the session for writing. Curses programs (vim, less, htop, …) enable
application cursor mode and expect `ESC O A`, not `ESC [ A`; sending the
normal-mode form is why arrow keys used to do nothing in those programs.

### 8.3 Key dispatch while attached

`handle_event` gets an early branch, before its existing `match key.code`:

```rust
if app.selected_block.is_shell() {
    return shell::handle_view_key(key, app)
}
```

`handle_view_key` is a small state machine:

```text
pending_kill = Some(id)? or pending_kill_all = Some(tab_id)?
  ├─ 'y' → kill that shell, or every shell of that tab (then the browser)
  └─ other → cancel the prompt (the key is swallowed)
shell_digits not empty?
  ├─ '0'..'9' → append a digit, commit when it can no longer grow
  ├─ Enter → commit the typed number
  ├─ Esc → cancel
  └─ other → cancel the number, then forward the key
prefix_active?
  ├─ 'd' → detach (browser comes back, session stays mounted)
  ├─ 'n' / 'p' → attach next / previous shell of the tab
  ├─ '0'..'9' → start typing a shell number (0-based)
  ├─ 'c' → create a new shell on this tab and attach it
  ├─ 'x' → ask to kill the attached shell (prompt in the status line)
  ├─ 'D' → ask to kill every shell mounted on this tab
  ├─ the prefix chord again → send a literal prefix byte
  └─ other → cancel the prefix, then forward the key (do not swallow typing)
prefix matches?
  └─ prefix_active = true (nothing is forwarded)
otherwise
  └─ write encode_key(key) to the PTY
```

`Esc` is **forwarded** to the shell (programs need it); detaching is `C-s d`.

### 8.4 Selecting a shell by number when a tab holds more than 10

Shells are numbered from **0** (tmux-style), and a tab may hold more than ten
of them, so a single digit is not always enough. The number is accumulated:

- `C-s 0` … `C-s 9` attach immediately when the tab has at most 10 shells.
- With more shells, the digits are collected in `app.shell_digits` and shown in
  the status line (`Shell #12  (Enter: attach, Esc: cancel)`). The number is
  committed when
  1. enough digits have been typed to cover the largest valid index
     (`(len - 1).to_string().len()`),
  2. appending another digit could not produce a valid index (e.g. `2` with 11
     shells), or
  3. `Enter` is pressed, or 450 ms pass without another digit.
- An out-of-range number clears the buffer instead of attaching the wrong
  shell.
- `tick_shell_digits` (called from the main loop) performs the timeout commit;
  the 450 ms constant is `DIGIT_COMMIT_DELAY` in `view.rs`.

The shell menu uses the same 0-based numbering and the same accumulation rule,
so `[10]` selects the eleventh shell of the tab.

### 8.5 Mouse events

The attached shell may enable mouse tracking at any time (`?9`, `?1000`,
`?1002`, `?1003`) and may also switch the report encoding (`?1005` UTF-8,
`?1006` SGR). The outer terminal is captured **only** while the attached
session asks for it:

| Child mode (`vt100`) | Outer capture |
| --- | --- |
| `None` | nothing |
| `Press` / `PressRelease` | `?1000h` |
| `ButtonMotion` | `?1002h` |
| `AnyMotion` | `?1003h` |

`?1006h` (SGR) is always enabled on the outer terminal, because crossterm then
reports releases and large coordinates reliably; every event is re-encoded for
the child. `sync_terminal_modes` runs once per main-loop iteration and writes
the sequence only when the state changes; `disable_terminal_modes` restores the
terminal (`?9l`, `?1000l`, `?1001l`, `?1002l`, `?1003l`, `?1005l`, `?1006l`,
`?1015l`, `?1016l`) on detach and on quit. Capturing only while the child wants
it keeps the browser, title and status line available for the terminal's own
text selection.

`handle_view_mouse`:

1. A `Down` is forwarded only inside `app.shell_area` (the rectangle recorded by
   `ui::ui` for the shell screen); presses and wheel events elsewhere are
   ignored.
2. A `Drag`/`Up` is forwarded if a button is currently held, with the
   coordinates clamped to the shell area, so a drag which leaves the shell (or
   a release over the title bar) does not get stuck.
3. `mouse_buttons` is a bitmask of the held buttons, reset on attach/detach and
   whenever the child changes the mouse mode.
4. Mouse activity cancels a pending prefix/number state; a pending kill
   confirmation swallows mouse events.

`encode_mouse` then:

- filters by mode: `Press` → presses + wheel only; `PressRelease` → no motion;
  `ButtonMotion` → no bare motion; `AnyMotion` → everything;
- maps buttons to xterm codes (left 0 / middle 1 / right 2, wheel 64–67), adds
  32 for motion and 4/8/16 for Shift/Alt/Ctrl;
- produces SGR (`ESC[<b;x;yM` / `m`), Default/X10 (`ESC[M` + three bytes; a
  release uses button 3 and coordinates are clamped to 223) or UTF-8 (`ESC[M` +
  three UTF-8-encoded values), matching the child's encoding;
- converts the outer 0-based terminal coordinates into 1-based coordinates
  relative to the top-left of the shell area.

### 8.6 Bracketed paste

When the attached child enables bracketed paste (`ESC[?2004h`, exposed by
`Screen::bracketed_paste()`), `sync_terminal_modes` enables it on the outer
terminal too, so crossterm delivers the clipboard as one `Event::Paste(text)`
instead of a stream of key events.

`handle_view_paste` reads the child's state at paste time:

- enabled → `ESC[200~` + the text verbatim + `ESC[201~`;
- disabled (race/fallback) → the text is typed: `\r\n`/`\n` become `\r`, and
  CJK/other characters keep their UTF-8 bytes.

The outer `?2004` is disabled on detach and on quit. Focus events are
deliberately not enabled or forwarded.

---

## 9. Actions: open / attach / detach / kill

New file `src/key_event/shell/view.rs`.

```rust
pub fn open_shell(app: &mut App) -> AppResult<()>;
pub fn attach(app: &mut App, id: ShellId) -> AppResult<()>;
pub fn attach_active(app: &mut App) -> AppResult<()>;
pub fn detach(app: &mut App);
pub fn next_shell(app: &mut App, forward: bool) -> AppResult<()>;
pub fn handle_view_key(key: KeyEvent, app: &mut App) -> AppResult<()>;
pub fn handle_view_mouse(mouse: MouseEvent, app: &mut App) -> AppResult<()>;
pub fn handle_view_paste(text: String, app: &mut App) -> AppResult<()>;

impl App<'_> {
    pub fn sync_terminal_modes(&mut self);
    pub fn disable_terminal_modes(&mut self);
    pub fn handle_shell_events(&mut self);
    pub fn tick_shell_digits(&mut self);
    pub fn commit_shell_digits(&mut self);
    pub fn clear_shell_digits(&mut self);
    pub fn remove_shell_from_tab(&mut self, tab_id: TabId, id: ShellId) -> Option<ShellId>;
    pub fn close_tab_shells(&mut self, tab_id: TabId) -> usize;
    pub fn close_other_shells(&mut self, tab_id: TabId, keep: ShellId) -> usize;
}
```

- `open_shell`: resolve `default_shell` (or `$SHELL`), `shell_sessions.open(...)`,
  push the new id onto `current_tab().shells`, set `active_shell`, then
  `attach`.
- `attach_active` (the `attach_shell` command, default key `B`): attach
  `current_tab().active_shell` without opening the shell menu. The tab's
  remembered shell follows every `attach` (numbers, `C-s n`/`C-s p`, the menu,
  the exit cleanup), so it is "the shell I was in last", not "the shell opened
  last". When the current tab has no shell at all it borrows the shell of
  another tab which was attached most recently (`App.shell_recent`, newest
  first, alive sessions only), with the same borrow-and-return behaviour as the
  menu; an error is reported only when no tab has a shell. `attach` still
  rejects a session which has already exited.
- `attach`: fail if the session is missing or dead. If the shell belongs to
  another tab, switch to that tab first (`tab::switch_to_tab`: save the current
  tab's view state, restore the target's) and remember the tab the user came
  from in `App.shell_return_tab`. This keeps the invariant that an attached
  shell belongs to the current tab, so the status line, `C-s c`, `C-s n/p`,
  `C-s D`, `C-s o` and the exit cleanup keep working without knowing about
  borrowing. Then clear `switch_case`, command
  line/expand/completion state, `shell_prefix_active`, `shell_scroll_mode`,
  `shell_pending_kill`, `shell_pending_kill_others`, `shell_pending_kill_all`;
  reset the shell's scrollback offset to the live screen; set
  `selected_block = Block::Shell(id)`; set `active_shell`; set
  `app.clear_screen = true`; record the shell in `App.shell_recent`.
- `detach`: clear the prefix/pending state, set
  `selected_block = Block::Browser(app.root())`, set `clear_screen`, then
  `refresh_after_shell`: re-read the current directory (the shell may have
  created, renamed or deleted files) and select the previously selected file
  again when it still exists. If the shell was borrowed from another tab,
  `App.shell_return_tab` makes `detach` switch back to it after the refresh (the
  shell's tab state is saved by that switch). Errors are reported through the
  error banner — note that a removed current directory is handled by
  `init_parent_files`, which moves the browser to an existing sibling. The
  session is **not** touched — it keeps running and keeps parsing output.
- `next_shell`: cycle `current_tab().shells` among alive sessions; no-op when
  there is only one.
- `x` (inside the prefix) requests a kill confirmation through
  `shell_pending_kill` (drawn in the status line, y/N), never through
  `SwitchCase` — a `SwitchCase` would replace the shell view and lose the
  terminal area until dismissed.
- `D` (inside the prefix) requests the same confirmation for **every** shell of
  the attached shell's tab through `shell_pending_kill_all: Option<TabId>`; the
  tab id is remembered instead of a boolean so the right tab is cleared even if
  the view changes while the prompt is open. Confirming calls
  `App::close_tab_shells(tab_id)` and leaves the browser behind (the view was
  pointing at a shell which no longer exists).
- `o` (inside the prefix) requests a confirmation to kill every **other** shell
  of the tab through `shell_pending_kill_others: Option<ShellId>` (the shell
  which is kept). The prompt reads `Kill N other shell(s)? (y/N)`; `y` calls
  `App::close_other_shells(tab_id, keep)`, which closes the other sessions,
  leaves `Tab.shells == [keep]` and keeps the view attached to it. With a single
  shell the key is a no-op (there is nothing to kill), so no prompt appears.
- `[` (inside the prefix) enters the **scroll mode** (`scroll::enter`): the
  viewport moves to the live screen and `shell_scroll_mode` is set, so the keys
  come from `keymap.toml`'s `shell_scroll_mode` (default `k`/`j`, `↑`/`↓`,
  `C-p`/`C-n`, `PageUp`/`PageDown`, `C-b`/`C-f`, `g`/`Home`, `G`/`End`,
  `q`/`Esc`/`Enter`) and every key is swallowed instead of being forwarded. The
  prefix chord is the one exception: it leaves the mode and arms
  `shell_prefix_active`, so the next key is the usual prefix command (`C-s d`,
  `C-s x`, … keep working after scrolling). The mode is not a separate buffer:
  `scroll::apply` only moves `vt100`'s scrollback offset
  (`Screen::set_scrollback`) and `set_mode` derives `shell_scroll_mode` from it,
  so new output keeps the viewport anchored (`vt100` increments the offset on
  scroll while it is non-zero) and reaching the bottom leaves the mode
  automatically.
- `scroll.rs` exposes `ScrollCommand` (`up`, `down`, `page up`, `page down`,
  `top`, `bottom`, `quit`), `enter`, `leave`, `apply` and `handle_wheel`. The
  page size is `shell_area.height - 1`; a wheel notch is three lines.
- `M-u` / `M-e` (the `shell_scroll` keys, checked before forwarding but after
  the prefix) scroll without entering the mode first; scrolling up enters it.
- `sync_terminal_modes` asks the outer terminal for exactly the child's mouse
  mode (via `outer_mouse_mode`), so a plain shell keeps the terminal's native
  wheel and drag-selection and a program which enabled mouse tracking gets its
  events forwarded. The only exception is the scroll mode: while it is open
  hire asks for at least `Press` reporting (a richer child mode is kept) so the
  wheel reaches it. Leaving the mode restores the child's mode/none, which
  gives the mouse back to the terminal.
- The wheel is part of the child's input by default: `handle_view_mouse`
  forwards it through `encode_mouse` like any other event, so `vim`, `less` and
  `htop` keep their own mouse scrolling. `Shift`+wheel (the terminal's modifier
  is kept in `MouseEvent::modifiers`) and every wheel while the scroll mode is
  open are hire's instead: they call `scroll::handle_wheel` and are not
  forwarded. With no mouse reporting the terminal never sends the wheel, so
  Shift+wheel cannot arrive; `C-s [` or `M-u` is the way in.
- `close_tab_shells` is the single place which clears a tab's shells: it
  detaches when the attached shell belongs to the tab, closes every session and
  clears `Tab.shells` / `Tab.active_shell`. The shell menu's `D`, the shell
  view's `C-s D`, closing a tab and replacing the tab list all go through it.
- `close_other_shells` is its counterpart for "keep exactly one": it closes
  every other session, detaches if the view pointed at one of them, and reduces
  `Tab.shells` to `[keep]` with `active_shell = Some(keep)`.

`AppCommand::Shell` and `--working-directory` both call `open_shell`, so the
`-w` mode becomes "open hire with a mounted shell": the user can still detach
and browse, and `exit` drops back to the browser exactly as today.

The existing `shell_process` stays untouched for everything that must remain a
one-shot foreground program: `:!<cmd>` (`command/cmd_utils.rs`),
`shell_command` from the keymap, `open_file_in_shell` (the file reader), and
`vim_diff`.

---

## 10. Main loop (`src/main.rs`)

Changes:

1. Drain exits and commit a timed-out shell number before drawing:
   ```rust
   app.handle_shell_events();          // drain_exits + reaping + view fix-up
   app.tick_shell_digits();            // commit a partial number after 450 ms
   if app.clear_screen {
       terminal.clear()?;
       app.clear_screen = false;
   }
   ```
2. Poll timeout: `16 ms` while a shell is attached, `200 ms` otherwise.
   PTY output is parsed by the reader thread; the short poll only makes hire
   repaint promptly and keeps typed input latency low.
3. Per-frame event drain: while a shell is attached, read up to N queued events
   per iteration (`while event::poll(Duration::ZERO)?`), otherwise keep the
   current one-event behaviour. Otherwise a paste would be limited to one key
   per 16 ms. The same loop also handles `Event::Mouse` (→ `handle_view_mouse`)
   and `Event::Paste` (→ `handle_view_paste`) when a shell is attached.
3b. Call `app.sync_terminal_modes()` after the shell events and before drawing:
   the child may enable/disable mouse tracking and bracketed paste at any time,
   and the outer terminal must follow. `app.disable_terminal_modes()` runs
   before `ratatui::restore()` so quitting never leaves the terminal in mouse
   capture or bracketed-paste mode.
4. Never treat `q` as "quit hire" while a shell is attached:
   ```rust
   let in_shell = matches!(app.selected_block, utils::Block::Shell(_));
   if !in_shell && key.code == KeyCode::Char('q') && key.modifiers.is_empty() { … }
   ```
   Recompute `in_shell` after each handled event, because the event may change
   the block.
5. Do not record macro keys while a shell is attached (they belong to the
   child).
6. Quit confirmation: also warn when mounted shells exist, e.g.
   `There're other tabs opened and 2 mounted shells will be killed, are you sure to quit? (y for yes)`.
7. `shell_in_workdir` calls `shell::open_shell(app)` instead of
   `shell_process(...)`. No terminal handle is needed: the session starts at
   24×80 and the first draw resizes it (the shell receives `SIGWINCH`).

`handle_shell_events` lives on `App` (`key_event/shell/view.rs`):

```rust
impl App<'_> {
    pub fn handle_shell_events(&mut self) {
        for id in self.shell_sessions.drain_exits() {
            let tab_id = self.shell_sessions.get(id).map(|s| s.tab);
            let was_attached = self.selected_block.shell_id() == Some(id);

            self.shell_sessions.close(id);

            let next = tab_id
                .and_then(|tab_id| self.remove_shell_from_tab(tab_id, id));

            if was_attached {
                match next {
                    Some(next) => {
                        if attach(self, next).is_err() {
                            detach(self);
                        }
                    },
                    None => detach(self),
                }
            }
        }
    }
}
```

View fix-up: if the attached shell exited, attach the tab's next live shell
(tmux closes a window and focuses the next one); if none is left, return to the
file browser.

---

## 11. Rendering

New file `src/ui/shell_block.rs`.

`ui::ui` gets a branch after `check_app_error` and before the
expanded-command-line handling and the browser layout:

```rust
if let Block::Shell(_) = app.selected_block {
    frame.render_widget(computer_info, title_layout[0]);
    frame.render_widget(item_num_info, title_layout[1]);
    app.shell_area = chunks[1];                 // mouse coordinate origin
    render_shell(app, frame, chunks[1])?;       // the shell area
    render_command_line(app, frame, chunks[2]); // shell status line
    return Ok(());
}
```

The shell keeps hire's title bar and state line; the PTY is sized to the middle
band, so full-screen programs lay out correctly.

### 11.1 Screen renderer

```rust
pub fn render_shell(app: &mut App, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
    let Some(id) = app.selected_block.shell_id() else { return Ok(()) };
    let Some(session) = app.shell_sessions.get_mut(id) else { return Ok(()) };

    // Only resizes when the size changed (parser.set_size + PTY ioctl).
    session
        .resize(area.height, area.width)
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    let parser = session.parser();              // MutexGuard<vt100::Parser>
    let screen = parser.screen();

    {
        let buf = frame.buffer_mut();

        for row in 0..area.height {
            for col in 0..area.width {
                let Some(cell) = screen.cell(row, col) else { continue };
                if cell.is_wide_continuation() {
                    continue;                   // the wide glyph covers this cell
                }

                if let Some(target) = buf.cell_mut((area.x + col, area.y + row)) {
                    // `contents()` is "" for a cleared cell; a real space must
                    // be written or crossterm prints nothing and the old
                    // character survives on the outer terminal.
                    if cell.has_contents() {
                        target.set_symbol(cell.contents());
                    } else {
                        target.set_symbol(" ");
                    }
                    target.set_style(style_of(cell));
                }
            }
        }
    }

    // The child's cursor belongs to the live screen; while the history is
    // shown it is not part of the viewport.
    if !screen.hide_cursor() && screen.scrollback() == 0 {
        let (row, col) = screen.cursor_position();
        frame.set_cursor_position((area.x + col, area.y + row));
    }
    Ok(())
}
```

`Screen::cell()` follows the scrollback offset (`vt100` 0.16's
`visible_cell`), so the same loop draws the history; only the cursor needs the
guard above.

`style_of` maps `vt100::Color::{Default, Idx, Rgb}` to
`ratatui::style::Color::{Reset, Indexed, Rgb}` and the cell's
bold/italic/underline/inverse flags to `Modifier`s. `Cell::is_wide()` /
`is_wide_continuation()` are provided by `vt100`; no extra crate is needed to
handle CJK.

**Blank cells must be written as a space.** `vt100::Cell::contents()` returns
`""` for a cleared cell (`Cell::clear` sets `len = 0`). Setting a ratatui cell's
symbol to `""` makes crossterm print nothing, so the character that used to be
at that position stays on the outer terminal — the "old content is not erased"
bug. The renderer therefore checks `Cell::has_contents()` and writes `" "` for
blank cells; the cell style is still applied so background colours survive.

### 11.2 Status line

`render_command_line` gains the arm

```rust
AppBlock::Shell(_) => {
    let lines = shell_status_lines(app);        // left + right
    if lines.is_empty() {
        return
    }
    frame.render_widget(StateLine::new(lines), area);
}
```

Left: the program running in the shell (`ShellSession::display_title` — `vim`
while vim owns the terminal, `bash` at the prompt), start directory, tab
ordinal (`[2/3]`). Right: **nothing at all in the idle state** — the hint is
only rendered once `shell_prefix` has been pressed, so an attached shell looks
like a plain terminal and the bottom-right corner stays free. The right line is
`PREFIX d: detach  n/p: switch  c: new  x: kill  D: all  o: others  [: scroll`
while `shell_prefix_active` is set; `SCROLL n  k/j: line  PgUp/PgDn: page
g/G: top/bottom  q: back` while `shell_scroll_mode` is set — the key labels are
taken from the configured `shell_scroll_mode` bindings
(`Keymap::shell_scroll_label`), so a remapped key is shown instead of the
default one; `Shell #N` while digits are being typed; `Kill shell "vim"?
(y/N)` while `shell_pending_kill` is set (the same display name); `Kill all N
shell(s) of this tab? (y/N)` while `shell_pending_kill_all` is set; `Kill N
other shell(s)? (y/N)` while `shell_pending_kill_others` is set; and an
application error while `shell_notice` is set. `shell_status_lines` returns a
one-element vector when there is no hint, which `StateLine` renders as a
left-only line.

`StateLine` (`ui/command_line.rs`) is already public and reusable.

### 11.3 Title bar

`get_title_right_info` keeps `|tab>|` and appends a shell badge when the current
tab has shells, e.g. `|1:2>|` (tab 1 has 2 mounted shells), and `SH 2/3` while a
shell is attached.

### 11.4 Images (kitty passthrough)

The screen renderer above only knows cells, so a TUI inside the shell cannot
make hire display a picture by itself. New file
`src/key_event/shell/graphics.rs`:

```rust
pub struct GraphicsSupport {
    pub kitty: bool,        // the real terminal speaks the kitty protocol
    pub cell: (u16, u16),   // cell size in pixels, from hire's own picker
}

pub enum GraphicsEvent {
    Reply(Vec<u8>),         // write to the child's PTY
    Forward(Vec<u8>),       // write to the real terminal
}

pub struct GraphicsFilter { /* pending APC, query tail, forwarding, ids */ }

impl GraphicsFilter {
    pub fn new(support: GraphicsSupport) -> Self;
    pub fn feed(&mut self, bytes: &[u8], rows: u16, cols: u16,
                text: &mut dyn FnMut(&[u8]),
                emit: &mut dyn FnMut(GraphicsEvent));
}
```

The reader thread runs every PTY chunk through `feed`:

- **Text** (everything except `ESC _ … ESC \` APC sequences) is fed to `vt100`
  exactly as before. APC sequences are removed because `vt100` has no image
  model and would drop them anyway.
- **Terminal queries** in the text stream are answered for the child:
  - the kitty graphics query (`a=q`): `OK` for direct transmission,
    `ENOTSUP` for shared memory / file media (the child then sends the data
    inline);
  - `CSI c` (DA1): `?62;22c` — no sixel, so a TUI which supports both chooses
    kitty;
  - `CSI 16t` / `CSI 14t`: the cell size / text area in pixels (from hire's
    picker). `vt100` still sees the query, which is harmless.
  - `CSI 5n`: `CSI 0n` (terminal ready).
- **Kitty image transmissions** (`a=T`/`a=t` with `U=1`, chunked with `m=1`)
  are collected and forwarded **once per image** to the real terminal, with
  `q=2` forced so the terminal never answers into hire's own stdin (an answer
  would be read as key presses). Only placeholder images are forwarded: a
  direct placement would draw at the child's cursor position, which hire does
  not own.
- **Deletes** are translated: `d=A` (the child's "delete everything") becomes
  one `d=I` per image id this shell forwarded, and a delete for an unknown id is
  dropped. A TUI inside a shell therefore cannot delete hire's own preview.

No decoding or re-encoding happens: the child sized the image for the real
cell size (that is what the `CSI 16t` answer is for), and the unicode
placeholder cells it paints are ordinary cells for `render_shell` (section
11.1), which writes their contents and foreground colour. Kitty decodes the
foreground colour as the image id and draws the image under those cells.

`App::handle_shell_graphics` (main loop, before the draw) drains the events,
writes replies to the child's PTY, forwards image data to stdout and requests a
full repaint. The repaint matters: the placeholder cells must be written after
the terminal knows the image. The event channel is bounded (`sync_channel`):
while hire is blocked in a foreground program nothing drains it, and an image
event which does not fit is dropped rather than queued forever (the child
repaints on its next preview change).

**Limitations:** only the kitty protocol is supported. Terminals whose images
are sixel or iTerm2-only, hire running inside tmux (the sequences would need
tmux passthrough wrapping), and TUIs which place each image cell by cell
(a fallback some programs use when they cannot identify the terminal brand)
keep the old behaviour: no picture inside the shell. `GraphicsSupport::disabled`
is what the child sees in those cases, so it simply does not use images.

---

## 12. Shell menu

New file `src/key_event/shell/menu.rs`, following the `SwitchCase` pattern used
by the tab menu.

- `shell_operation(app)` opens the page with
  `SwitchCaseData::Struct(ShellMenuState)` (`{ deleting: bool, selecting:
  String, clear_pending: Option<TabId> }`).
- Message: the shells which can be attached, numbered from **0**. The current
  tab's shells come first (their numbers are the ones users already know), then
  the shells of every other tab, grouped under `Tab N` (1-based, like the title
  badge and `switch_tab`). A line reads `  [0] * vim  ~/project`; `*` marks the
  shell's own tab's active shell, `!` marks an exited one. The name is
  `ShellSession::display_title` — the program currently running in that shell,
  not the shell program it was started with. Without it every shell started
  from the same `default_shell` would read `zsh` even when one of them is
  running vim. A shell at its prompt shows the shell's own name.
- Keys:
  - `0..9` — attach that shell. The menu builds one flat list (`menu_entries`:
    current tab first, then the other tabs in tab order). A shell of another
    tab is **borrowed**: `attach` switches that tab internally (the view must
    belong to the shell's tab, see §9), stores `shell_return_tab`, and `detach`
    refreshes the shell's tab and returns to the tab the menu was opened on.
    A number is accumulated exactly like the prefix's digits (0-based, width of
    the largest index); the page is re-rendered while the number is incomplete.
    Attaching clears the switch case and returns `false`, so `switch_match`
    does not overwrite the block with `quit_command_mode`.
  - `n` — `open_shell` on this tab and attach it.
  - `x` then the number — kill that shell. The scope is the **current tab**,
    even though the other tabs' shells are listed: `selected_index` is fed
    `current_tab().shells.len()`, so a number of another tab is out of range
    and does nothing. Two-step, no modal confirm needed.
  - `D` — kill **every** shell mounted on the current tab; the tab itself stays
    open. The page turns into a y/N confirmation (`Kill all N shell(s) of tab
    X? (y/N)`; the tab number keeps the prompt unambiguous now that the other
    tabs are visible); `y` calls `App::close_tab_shells(tab_id)` and any other
    key cancels. This is the tab-level "clear shells" action; it used to live in
    the tab menu and was moved here because it is a shell operation, not a tab
    one. The attached view's `C-s D` performs the same action while a shell owns
    the input.
  - `Esc` — close (handled by the existing `Esc` path).
- Rebuilding the page after a kill reuses `generate_msg`; when the tab has no
  shells left, return `true` to close the menu. After clearing the shells the
  menu is shown again (the current tab section reads `(no shell)`), so the tab
  is visibly still there.
- `menu_entries` and `generate_msg` must walk the tabs in the same order
  (current first, then `TabList::tabs()` order) so the displayed numbers and the
  attached shells agree. Other tabs are only listed when they have at least one
  shell; the current tab is always shown (reading `(no shell)` when empty).

Add the command to `AppCommand` (`ShellMenu`), parse `"shell_menu"` in
`AppCommand::from_str`, dispatch it in `AppCommand::execute`
(`shell::menu::shell_operation(app)?`), and bind it in both shipped keymaps:

```toml
{ key = "T", run = "shell_menu" },   # List / attach / kill shells of any tab
```

`T` is free in `default.toml` and `colemak.toml`. `spawn_shell` (`S`) keeps its
current key and now creates + attaches a new session.

The menu's job for the *common* case — "go back to the shell I was in" — does
not need a list: the tab already stores `active_shell`. `AppCommand::ShellAttach`
(parsed from `"attach_shell"`, bound to `B` in both shipped keymaps) calls
`shell::attach_active`, which attaches `current_tab().active_shell`; when the
tab has no shell it borrows the most recently attached shell of another tab
(`App.shell_recent`, newest first, alive sessions only), with the same
borrow-and-return behaviour as the menu. The remembered value is updated by
every attach, so it is the last shell used, not the last one opened.

```toml
{ key = "B", run = "attach_shell" },   # Attach the shell this tab used last
```

---

## 13. Configuration

New `user_config.toml` option:

```toml
# The prefix chord used inside an attached shell. Default: "C-s".
shell_prefix = "C-s"
```

Wiring:

- `config/types.rs`: `default_value` → `"shell_prefix" => "C-s"`; `value_from`
  treats it like the other string options.
- `config/mod.rs`: add `"shell_prefix"` to the `configs` array; after
  `init_user_config`, parse it into `app.shell_prefix` and report a bad value
  through the existing `AppError` collection (keep the default).
- `App::default` initialises `ShellPrefix::default()` so a broken user config
  never prevents startup.

### Scroll keys (`keymap.toml`)

The two arrays live in `keymap.toml`, not in `user_config.toml`, because they
are key bindings like the rest of that file:

```toml
# Keys which scroll while the shell is live (defaults shown).
shell_scroll = [
    { key = "M-u", run = "scroll up"   },
    { key = "M-e", run = "scroll down" },
]

# Keys of the scroll mode (since `vt100` 0.16 can page the whole history).
shell_scroll_mode = [
    { key = "k",      run = "scroll up"        },
    ...
    { key = "q",      run = "scroll quit"      },
]
```

- `KeySpec` (`key_event/shell/input.rs`) parses the `key` value: a character, an
  optional `C-`/`M-`/`S-` prefix and a named key (`Up`, `PageUp`, `Esc`,
  `Enter`, `F5`, …). An uppercase character implies `SHIFT`, matching what the
  terminal reports.
- `ScrollCommand::from_str` accepts `scroll up`, `scroll down`, `scroll page
  up`, `scroll page down`, `scroll top`, `scroll bottom` and `scroll quit`.
- The defaults are built into `Keymap::default()`, so an older `keymap.toml`
  without the sections keeps working; a file entry replaces the binding of the
  same key and leaves the other defaults in place.

No other configuration is required: shell program, GUI commands and file reader
keep their existing options.

**Dev config filenames.** While a feature is being developed, the deliberate
toggle in `config/mod.rs::get_conf_file` may make the app read
`user_config_dev.toml` / `keymap_dev.toml` instead of the production names; see
`docs/ARCHITECTURE.md` §6, "Dev config filenames". When preparing a session for
the shell menu/harness, set `default_shell` in whichever file the active block
selects, and create the matching keymap file (`keymap_dev.toml` or
`keymap.toml`); the app panics in `init_keymap` if the keymap file is missing.
Do not flip the block back just to run a harness.

---

## 14. File-by-file change list

| File | Change |
| --- | --- |
| `Cargo.toml` | `portable-pty = "0.9"`, `vt100 = "0.16"`, `ratatui = "0.30"`, `ratatui-image = "10"` (no default features: no `chafa`), `ansi-to-tui = "8"` |
| `src/utils/types.rs` | `ShellId`; `Block::Shell(ShellId)`; `Block::is_shell()` / `Block::shell_id()` |
| `src/key_event/shell/pty.rs` *(new)* | `PtyProcess` (spawn with the pixel size, reader thread through the graphics filter, write, resize, kill + reap) |
| `src/key_event/shell/graphics.rs` *(new)* | `GraphicsSupport`, `GraphicsEvent`, `GraphicsFilter` (kitty APC splitting, query answers, image forwarding and delete translation) |
| `src/key_event/shell/session.rs` *(new)* | `ShellMeta`, `ShellSession`, `ShellSessions`; the graphics event channel and `drain_graphics` |
| `src/key_event/shell/input.rs` *(new)* | `KeySpec` (key + modifiers, parsed from `M-u`/`C-b`/`PageUp`/…), `ShellPrefix`, `encode_key`, `encode_paste` |
| `src/key_event/shell/mouse.rs` *(new)* | `encode_mouse`, `button_bit`, `inside` |
| `src/key_event/shell/view.rs` *(new)* | `open_shell` (with `graphics_support`), `attach` (switches to the shell's tab and records `shell_return_tab` when it is borrowed), `attach_active` (the `attach_shell` command, with the most-recent-other-tab fallback), `detach` (refreshes the browser, then returns to `shell_return_tab`), `next_shell`, `handle_view_key` (`C-s x`, `C-s D`, `C-s o`, `C-s [`, the `shell_scroll` keys), `handle_view_mouse` (wheel forwarded by default, `Shift`+wheel and the scroll mode scroll the history; `outer_mouse_mode` capture policy), `handle_view_paste`; the `App` methods `note_attached_shell`, `handle_shell_events`, `handle_shell_graphics`, `tick_shell_digits`, `commit_shell_digits`, `clear_shell_digits`, `remove_shell_from_tab`, `close_tab_shells`, `close_other_shells`, `sync_terminal_modes`, `disable_terminal_modes` |
| `src/key_event/shell/scroll.rs` *(new)* | `ScrollCommand`, `enter`, `leave`, `apply`, `handle_wheel`: moves `vt100`'s scrollback offset and derives `shell_scroll_mode` from it |
| `src/key_event/shell/menu.rs` *(new)* | `shell_operation` + `SwitchCase` state/message; `menu_entries` (current tab first, then the other tabs) and the grouped message; digits attach any tab's shell (a foreign one is borrowed), `n`/`x`/`D` stay on the current tab; `D` (clear every shell of the tab, with a y/N confirmation naming the tab) |
| `src/key_event/shell/mod.rs` | declare the new modules, re-export the public API |
| `src/key_event/shell/utils.rs` | keep `shell_process`; factor shell-program resolution into a helper |
| `src/key_event/tab/types.rs` | `TabId`, `Tab`, `TabList` rewrite (§5) |
| `src/key_event/tab/utils.rs` | migrate every mutation site; every tab removal (`c`, `d` + number, `o`, stored-group restore) asks first when mounted shells would be killed (`TabAction` + confirmation page); show shell counts. `switch_to_tab`/`switch_to` save the current tab's view state and restore the target's; they are used by `next`/`prev`/`switch_tab` and by the shell borrow. Clearing the shells of a tab without removing it is not a tab action anymore: it lives in the shell menu (`T`, `D`) |
| `src/key_event/tab/mod.rs` | export `TabId`, `Tab` |
| `src/key_event/sudo_session.rs` | adapt to `tabs()` / `set_tabs`; shells are never transferred |
| `src/app/mod.rs` | `shell_sessions`, `shell_prefix`, `shell_prefix_active`, `shell_scroll_mode`, `shell_pending_kill`, `shell_pending_kill_others`, `shell_pending_kill_all`, `shell_recent`, `shell_return_tab`, `shell_digits`, `shell_digit_deadline`, `clear_screen`, `shell_notice`, `outer_mouse_mode`, `outer_bracketed_paste`, `mouse_buttons`, `shell_area` |
| `src/main.rs` | drain exits + `tick_shell_digits` + `sync_terminal_modes` + `handle_shell_graphics`, `clear_screen`, poll timeout, key/mouse/paste event drain, `q` guard, quit warning, `-w` mode, `disable_terminal_modes` before restore |
| `src/app/image_preview.rs` | `protocol_type` / `font_size` accessors (the shell graphics support is derived from the detected protocol and cell size) |
| `src/key_event/mod.rs` | early `Block::Shell` dispatch; `AppCommand::Shell` → `open_shell`; `ShellMenu`; `ShellAttach` → `attach_active`; exports |
| `src/command/types.rs` | `AppCommand::ShellMenu` + `"shell_menu"`; `AppCommand::ShellAttach` + `"attach_shell"` |
| `src/config/types.rs`, `src/config/mod.rs` | `shell_prefix` option |
| `src/config/keymap.rs` | the `shell_scroll` / `shell_scroll_mode` arrays, their defaults, `ScrollCommand` parsing and `shell_scroll_label` |
| `src/ui/mod.rs` | shell-view branch; title badge; records `shell_area` |
| `src/ui/shell_block.rs` *(new)* | screen renderer (scrollback-aware, cursor only on the live screen) + status line (prefix hint only while the prefix is active; scroll-mode hint built from the configured keys; digits/kill prompt/notice) |
| `src/ui/command_line.rs` | `Block::Shell` arm (reuses `StateLine`) |
| `keymap_config/default.toml`, `colemak.toml` | `shell_menu` / `attach_shell` bindings; `shell_scroll` and `shell_scroll_mode` sections |
| `README.md`, `docs/ARCHITECTURE.md` | user docs and architecture docs (§16) |

Exhaustive `match` sites the compiler will point at when `Block::Shell` is
added (all are unreachable after the early dispatch; `Esc` already has a `_`
arm, `Up` / `Down` in `key_event/mod.rs` need one): `key_event/mod.rs`
(`Up`, `Down`) and `ui/command_line.rs` (`render_command_line`, which renders
the shell status line). Everything else matches with `if let` or `_`.

---

## 15. Edge cases

| Situation | Behaviour |
| --- | --- |
| Shell exits while attached | reap it, attach the tab's next live shell, else return to the browser and refresh the current directory (and, if the shell was borrowed, return to the tab it was borrowed from); tab list updated |
| Detach (`C-s d`) | return to the browser and re-read the current directory, keeping the selected file selected; a borrowed shell's tab is refreshed first and `detach` then switches back to the tab the user came from |
| Shell killed (`C-s x`, no shell left) | same as detaching: back to the browser, refreshed (and back to the original tab when the shell was borrowed) |
| Attach a shell of another tab (menu digit, or `B` when the current tab has none) | `attach` switches that tab internally (`switch_to_tab`) and stores `App.shell_return_tab`; while attached the title bar shows the shell's tab. `detach` refreshes that tab and switches back to the original one |
| `B` on a tab without any shell | borrow the most recently attached shell of another tab (`App.shell_recent`, alive only); the same borrow-and-return applies. An error is shown only when no tab has an attachable shell |
| The borrowed shell's tab is removed while it is attached | impossible through the UI (no tab command is reachable while a shell owns the input); if `shell_return_tab` no longer exists, `detach` reports the tab-switch error and stays in the shell's tab |
| Shell exits while detached | remove it from `shells`; clear `active_shell` if it pointed at the shell; no view change |
| Close a tab with shells | the tab menu asks first; `y` closes the tab and kills every shell of it (`close_tab_shells`) |
| `delete_other_tabs` | asks first when a removed tab has shells; `close_tab_shells` on every removed tab |
| Apply stored tabs (`storage_tabs`) | asks first when any mounted shell would be killed; then every current tab is cleared and replaced; sessions are never persisted |
| Quit hire | warn when live shells exist; dropping `ShellSessions` drops every `PtyProcess`, which kills and reaps the children |
| Sudo session | tabs are handed over without shells; the parent's shells keep running while the root instance is open, and are restored on return; the root instance cannot see or attach them |
| Terminal resize | `render_shell` compares the shell area with the PTY size and calls `Parser::set_size` + `MasterPty::resize` (which raises `SIGWINCH`) |
| Error while attached | show it in the shell status line instead of `check_app_error` hijacking the block to `CommandLine` (small guard in `check_app_error`) |
| Prefix inside tmux | outer tmux may swallow `C-b`, and some terminals use `C-s` for flow control; the option exists so the user can set another chord |
| Background output while a foreground process runs (`:!vim`) | reader threads keep parsing into the shared screen; only exit ids travel through a channel, so no unbounded buffering |
| Shell program missing | `AppError` from `open_shell`, shown in the browser; no session is registered |
| Blank / cleared shells cells | written as a real space, so erases reach the outer terminal (see §11.1) |
| Curses program in the shell | arrows/Home/End follow the child's DECCKM state, so vim/less/htop receive the form they expect |
| Program running in a shell (`vim`, `htop`, `less`, …) | the shell menu and the status line show its name (the PTY's foreground process group, `/proc/<pgid>/comm`); the shell's own name returns when it is back at the prompt |
| `/proc` unreadable or the foreground process already exited | `foreground_title` returns `None` and the session is labelled with the shell program (`meta.title`); no error is reported |
| Dead session | `foreground_title` returns `None`, so no `/proc` lookup is done and the label falls back to `meta.title` |
| TUI paints an image (kitty graphics) | the image data is forwarded to the real terminal (section 11.4) and the placeholder cells the TUI paints make it visible; the shell screen itself remains text |
| Terminal without kitty support, or hire inside tmux | the child gets no graphics answers, so it does not use images (the previous behaviour) |
| TUI uses sixel, iTerm2 images or per-cell kitty placements | not forwarded; only placeholder-style kitty transmissions are supported (section 11.4) |
| TUI deletes its images (`d=A`) | translated to one delete per forwarded image id, so hire's own preview image is never touched |
| Image transmitted while the shell is detached | forwarded anyway (the terminal caches it); attaching repaints the placeholder cells |
| Terminal query from a background shell | answered on the next main-loop iteration by writing to the child's PTY |
| `C-s D` while attached | a `Kill all N shell(s) of this tab?` prompt; `y` kills every shell of the tab and returns to the browser, any other key cancels |
| `C-s o` while attached | a `Kill N other shell(s)?` prompt; `y` kills the tab's other shells, keeps the attached one attached and leaves `Tab.shells == [keep]`; any other key cancels; with a single shell the key does nothing |
| Clear a tab's shells from the shell menu (`T`, `D`) | the menu asks (`Kill all N shell(s) of tab X? (y/N)`); `y` keeps the tab and empties its shell list, then shows the shell menu again. `D` (and `x`) only ever touch the tab the menu was opened on, even though other tabs' shells are listed |
| Idle shell view | the status line's right side is empty; the prefix hint appears only after `shell_prefix` is pressed |
| Close/delete a tab, close other tabs, restore a stored group | asks first only when the tabs which would lose shells actually have some; `y` performs it, any other key returns to the tab menu |
| Mouse enabled by the child | outer mouse capture is enabled in the matching tracking mode + SGR; wheel, clicks and drags are re-encoded for the child's mode/encoding (its own wheel scrolling keeps working) |
| Mouse disabled by the child | no mouse reporting is enabled, so the terminal's wheel and drag-selection work natively; hire never sees those events |
| `Shift` + wheel | hire's: it scrolls the history (entering the scroll mode) instead of forwarding, when the event reaches hire (child mouse mode or scroll mode open) |
| Scroll mode open | `Press` reporting is enabled even if the child did not ask, so the wheel scrolls the history freely; clicks are ignored; leaving the mode restores the child's mode |
| Detach / quit | outer capture is disabled; a half-tracked drag is reset |
| Mouse press on title or status line | ignored; a drag/release which started inside is clamped to the shell area |
| Child enables bracketed paste | outer `?2004h` is set; a paste is forwarded as one bracketed paste |
| Paste while the scroll mode is open | ignored; the keys are hire's until `q`/`Esc` (or scrolling to the bottom) |
| Scroll up (`M-u`, `Shift`+wheel, `C-s [` + `k`) | the viewport moves into `vt100`'s scrollback; `shell_scroll_mode` is on, so the keys no longer reach the shell |
| New output while scrolled back | `vt100` keeps the viewport on the same lines (the offset grows), so the history does not jump |
| Scrolling down to the bottom | the offset reaches 0 and the mode closes on its own; the keys and the wheel are the shell's again |
| Resize while scrolled back | `Screen::set_size` keeps the offset; the renderer shows the history at the new width |
| Full-screen program (`vim`, `htop`) | it runs on `vt100`'s alternate screen, which has no history: `C-s [` shows `SCROLL 0` and `q` returns |
| `C-s d`/`C-s x`… while scrolled back | the prefix chord leaves the scroll mode and arms the prefix, so the next key runs the usual prefix command |

---

## 16. Documentation updated with the implementation

- `README.md`: new "Shell Sessions" section (spawn → detach → menu → quota of
  shells per tab), `shell_prefix`, the prefix table (including `C-s o`), the
  fact that the prefix hint only appears after the prefix is pressed, the shell
  menu's `D`, the fact that quitting kills the shells, the cross-tab shell
  list / borrow-and-return of `T`, and `B`'s fallback to the most recently
  attached shell of another tab.
- `docs/ARCHITECTURE.md`:
  - §1 event-loop step: drain shell exits, `clear_screen`, variable poll.
  - §3 module table: the six new `key_event/shell/*` files and
    `ui/shell_block.rs`.
  - §3 `Block` description: add `Shell(ShellId)`.
  - §3 `App` field list: `shell_sessions`, `shell_prefix`, `shell_prefix_active`,
    `shell_pending_kill`, `shell_pending_kill_others`,
    `shell_pending_kill_all`, `shell_recent`, `shell_return_tab`,
    `shell_digits`, `shell_digit_deadline`,
    `clear_screen`, `shell_notice`, `outer_mouse_mode`, `outer_bracketed_paste`,
    `mouse_buttons`, `shell_area`.
  - §3 module table: `menu_entries` / the grouped menu, `attach`'s tab borrow,
    `detach`'s return, and `tab::switch_to_tab`.
  - §3 `key_event/tab/types.rs`: `Tab` / `TabId`.
  - §4: a "Shell session" flow (spawn → attach → detach → menu → exit/close)
    and an "Images inside a shell" flow; the menu block includes the other
    tabs, and the borrow path shows `shell_return_tab` and the return.
  - §5: this document.
  - §6 test notes: the in-memory graphics filter tests.
- `keymap_config/{default,colemak}.toml`: `shell_menu` and `attach_shell`
  lines, and the `shell_scroll` / `shell_scroll_mode` arrays with comments.
- `README.md`: the scrollback keys, the `shell_scroll`/`shell_scroll_mode`
  sections of `keymap.toml`, the mouse-capture/`Shift` note, and the `B`
  ("back to the shell this tab used last") shortcut.
- `Cargo.toml` dependency list in §1 of `ARCHITECTURE.md` (`portable-pty`,
  `vt100` 0.16, `ratatui` 0.30, `ratatui-image` 10, `ansi-to-tui` 8) and the
  reason for the ratatui upgrade (`unicode-width`).

---

## 17. Phases

| Phase | Content | Status |
| --- | --- | --- |
| 0 | `Tab`/`TabId` refactor, no behaviour change | done |
| 1 | PTY + reader + screen renderer + `Block::Shell` + prefix + attach/detach + `spawn_shell` / `-w` wiring | done |
| 2 | Shell menu, tab badges, kill/quit confirmations, `shell_prefix` config, sudo/storage/quit edge cases | done |
| 2b | Mouse forwarding (4 tracking modes × 3 encodings) and bracketed paste; outer mode sync | done |
| 2c | Images painted by a TUI inside a shell: kitty query answers, placeholder image passthrough, pixel size in the PTY | done (kitty terminals only) |
| 2d | Scrollback: `vt100` 0.16, `C-s [` scroll mode with configurable keys, `M-u`/`M-e`, wheel, status hint | done |
| 2e | Quick attach: `attach_shell` (`B`) attaches the tab's `active_shell` without the menu | done |
| 2f | Cross-tab shells: the menu lists every tab's shells, a foreign shell is borrowed (`shell_return_tab`), `B` falls back to the most recently attached shell, `detach` refreshes and returns | done |
| 3 | Rename, activity notification, optional split panes | not implemented |

Phase 1 is the usable MVP: `S` opens a shell, `C-s d` mounts it in the
background, `C-s n/p` cycles the tab's shells, `S` from the browser opens
another one. Phases 0–2f are implemented; phase 3 is not.

---

## 18. Alternatives considered

1. **Shell out to tmux.** `spawn_shell` would run
   `tmux new-session -d -s hire-<pid>-<n> -c <cwd>` and store the session name
   in the tab; attach with `tmux attach-session`; tmux's own `C-b d` detaches.
   This is far less code (~250 lines) and gives perfect terminal fidelity, but
   adds a runtime dependency on tmux, breaks when hire itself runs inside tmux,
   and makes hire a launcher rather than the multiplexer. It is a viable
   fallback if the native work is rejected.
2. **Suspend/resume with `SIGTSTP` + `tcsetpgrp`.** The shell cannot keep
   running while detached (it would hit `SIGTTIN`/`SIGTTOU` as a background
   process group), so this does not satisfy "keeps running in the background".
3. **`tui-term` for rendering.** `tui-term 0.2` is pinned to ratatui 0.29 +
   vt100 0.15; the renderer above is ~100 lines and keeps us on current vt100
   without another dependency (and it also gives the scrollback for free).
4. **Borrowed view without switching tabs.** Attaching a shell of another tab
   while keeping `current_tab()` on the origin would avoid the internal switch,
   but the status line (`[pos/total]`), `C-s c`, `C-s n/p`, `C-s D`/`C-s o` and
   the exit cleanup all read `current_tab()`. They would each need the shell's
   owner tab instead, and `C-s c` would become ambiguous (create on the shell's
   tab or on the one being browsed?). Switching the tab for the duration of the
   view and returning on `detach` keeps one invariant and is much smaller.
