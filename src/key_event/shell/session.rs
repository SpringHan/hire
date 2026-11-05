// Shell sessions.
//
// A session is a PTY-backed shell plus its parsed screen. Sessions belong to a
// tab and survive being detached: the reader thread keeps consuming and
// parsing output while the file browser is shown.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender, SyncSender},
        Arc, Mutex, MutexGuard,
    },
};

use portable_pty::PtySize;

use crate::{error::AppResult, key_event::TabId, utils::ShellId};

use super::{
    graphics::{GraphicsEvent, GraphicsSupport},
    pty::{PtyGraphics, PtyProcess},
};

/// The size a session is spawned with; the first draw resizes it to the real
/// terminal area.
pub const DEFAULT_PTY_ROWS: u16 = 24;
pub const DEFAULT_PTY_COLS: u16 = 80;

/// How many scrolled-off lines are kept for every session.
pub const SCROLLBACK_ROWS: usize = 240;

/// How many graphics events may wait for the main loop.
const GRAPHICS_QUEUE: usize = 32;

pub struct ShellMeta {
    /// The basename of the shell program the session was started with, e.g.
    /// `bash`. This is not necessarily what is running in the session right
    /// now; see [`ShellSession::display_title`].
    pub title: String,

    /// The directory the shell was started in.
    pub cwd: PathBuf,
}

pub struct ShellSession {
    pub tab: TabId,
    pub meta: ShellMeta,
    pub alive: bool,

    screen: Arc<Mutex<vt100::Parser>>,
    pty: PtyProcess,
}

impl ShellSession {
    /// Lock and return the parsed screen.
    pub fn parser(&self) -> MutexGuard<'_, vt100::Parser> {
        // The reader thread never panics while holding the lock, but a poisoned
        // mutex must not take the whole application down.
        self.screen.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn write(&mut self, bytes: &[u8]) -> AppResult<()> {
        self.pty.write(bytes)
    }

    /// The name of the program which currently owns the session's terminal.
    ///
    /// `Some("vim")` while vim runs in the shell, `Some("zsh")` at the prompt.
    /// `None` when it cannot be determined (the session is dead, or `/proc`
    /// is unreadable), so the caller falls back to the shell name.
    pub fn foreground_title(&self) -> Option<String> {
        if !self.alive {
            return None
        }

        self.pty.foreground_pid().and_then(process_name)
    }

    /// The name to show for this session: the program running in it, falling
    /// back to the shell program it was started with.
    ///
    /// A mounted shell whose own name is shown is indistinguishable from every
    /// other mounted shell of the same shell program; the foreground program
    /// (`vim`, `htop`, …) is what the user needs to tell them apart.
    pub fn display_title(&self) -> String {
        self.foreground_title()
            .unwrap_or_else(|| self.meta.title.clone())
    }

    /// Keep the parser and the PTY at the same size.
    ///
    /// The size is only applied when it changed: `vt100` resets the wrapped
    /// state of every row on `set_size`, even for an identical size, and the
    /// renderer calls this on every frame.
    pub fn resize(&mut self, rows: u16, cols: u16) -> AppResult<()> {
        let rows = rows.max(1);
        let cols = cols.max(1);

        {
            let mut parser = self.parser();

            if parser.screen().size() != (rows, cols) {
                parser.screen_mut().set_size(rows, cols);
            }
        }

        self.pty.resize(rows, cols)
    }
}

/// The name of `pid` as the kernel reports it (`vim`, `nvim`, `htop`, …).
///
/// `/proc/<pid>/comm` is what a process list shows; programs may change it and
/// the kernel truncates it to 15 bytes. `None` when it cannot be read, e.g. on
/// a platform without `/proc`, so the caller falls back to the shell name.
fn process_name(pid: u32) -> Option<String> {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = name.trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

pub struct ShellSessions {
    sessions: HashMap<ShellId, ShellSession>,
    next_id: u64,
    exit_tx: Sender<ShellId>,
    exit_rx: Receiver<ShellId>,
    graphics_tx: SyncSender<(ShellId, GraphicsEvent)>,
    graphics_rx: Receiver<(ShellId, GraphicsEvent)>,
}

impl Default for ShellSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellSessions {
    pub fn new() -> Self {
        let (exit_tx, exit_rx) = mpsc::channel();
        // Bounded: while hire is blocked in a foreground program nobody drains
        // the graphics, and image events are large. Dropping one is harmless
        // (the child repaints on its next preview change), growing forever is
        // not.
        let (graphics_tx, graphics_rx) = mpsc::sync_channel(GRAPHICS_QUEUE);

        Self {
            sessions: HashMap::new(),
            next_id: 0,
            exit_tx,
            exit_rx,
            graphics_tx,
            graphics_rx,
        }
    }

    /// Spawn a new shell and return its id.
    ///
    /// `graphics` is what the new terminal tells the child about image support;
    /// the PTY's pixel size is derived from it so programs which read the
    /// window size in pixels scale their images correctly.
    pub fn open(
        &mut self,
        tab: TabId,
        program: &str,
        cwd: &Path,
        graphics: GraphicsSupport,
    ) -> AppResult<ShellId>
    {
        let id = ShellId(self.next_id);
        self.next_id += 1;

        let parser = vt100::Parser::new(
            DEFAULT_PTY_ROWS,
            DEFAULT_PTY_COLS,
            SCROLLBACK_ROWS,
        );
        let screen = Arc::new(Mutex::new(parser));

        let cell = graphics.cell;
        let pty = PtyProcess::spawn(
            program,
            cwd,
            PtySize {
                rows: DEFAULT_PTY_ROWS,
                cols: DEFAULT_PTY_COLS,
                pixel_width: DEFAULT_PTY_COLS.saturating_mul(cell.0),
                pixel_height: DEFAULT_PTY_ROWS.saturating_mul(cell.1),
            },
            screen.clone(),
            self.exit_tx.clone(),
            PtyGraphics {
                support: graphics,
                events: self.graphics_tx.clone(),
            },
            id,
        )?;

        let title = Path::new(program)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(program)
            .to_owned();

        self.sessions.insert(
            id,
            ShellSession {
                tab,
                meta: ShellMeta {
                    title,
                    cwd: cwd.to_owned(),
                },
                alive: true,
                screen,
                pty,
            },
        );

        Ok(id)
    }

    pub fn get(&self, id: ShellId) -> Option<&ShellSession> {
        self.sessions.get(&id)
    }

    pub fn get_mut(&mut self, id: ShellId) -> Option<&mut ShellSession> {
        self.sessions.get_mut(&id)
    }

    pub fn is_alive(&self, id: ShellId) -> bool {
        self.sessions.get(&id).map(|s| s.alive).unwrap_or(false)
    }

    pub fn alive_count(&self) -> usize {
        self.sessions.values().filter(|s| s.alive).count()
    }

    /// Take the graphics events produced since the last call.
    pub fn drain_graphics(&mut self) -> Vec<(ShellId, GraphicsEvent)> {
        let mut events = Vec::new();

        while let Ok(event) = self.graphics_rx.try_recv() {
            events.push(event);
        }

        events
    }

    /// Take the ids of every shell which exited since the last call.
    pub fn drain_exits(&mut self) -> Vec<ShellId> {
        let mut ids = Vec::new();

        while let Ok(id) = self.exit_rx.try_recv() {
            ids.push(id);
        }

        ids
    }

    /// Kill and remove one session.
    pub fn close(&mut self, id: ShellId) {
        if let Some(mut session) = self.sessions.remove(&id) {
            session.alive = false;
            session.pty.kill();
        }
    }

    pub fn close_many(&mut self, ids: &[ShellId]) {
        for id in ids {
            self.close(*id);
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn test_process_name_of_the_test_process() {
        let name = process_name(std::process::id()).expect("the test process has /proc entry");

        assert!(!name.is_empty());
    }

    /// A session at its prompt reports the shell itself; once a program runs in
    /// it, the foreground program is reported instead. That name is what the
    /// shell menu and the status line show.
    #[test]
    fn test_display_title_follows_the_foreground_program() {
        // `/bin/sh` and `/proc` are the only assumptions; skip elsewhere.
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut sessions = ShellSessions::new();
        let id = sessions
            .open(
                TabId(0),
                "/bin/sh",
                Path::new("/"),
                GraphicsSupport::disabled(),
            )
            .unwrap();
        let session = sessions.get_mut(id).unwrap();

        // The shell has to start, read the line and put `sleep` in the
        // foreground process group, so poll instead of sleeping a fixed time.
        session.write(b"sleep 5\n").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut title = session.display_title();

        while title != "sleep" && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            title = session.display_title();
        }

        assert_eq!(title, "sleep");
    }

    /// A program which asks the terminal where the cursor is gets an answer,
    /// and the answer is the child screen's position. `gh`'s text prompts block
    /// (and swallow what is typed) while they wait for this report.
    #[test]
    fn test_cursor_position_query_is_answered() {
        if !Path::new("/bin/sh").exists() {
            return
        }

        let mut sessions = ShellSessions::new();
        let id = sessions
            .open(
                TabId(0),
                "/bin/sh",
                Path::new("/"),
                GraphicsSupport::disabled(),
            )
            .unwrap();

        // Move the cursor to row 5, column 9 and ask where it is. The report is
        // one-based: `CSI 5 ; 9 R`.
        sessions
            .get_mut(id)
            .unwrap()
            .write(b"printf '\\033[5;9H\\033[6n'\n")
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let answer = b"\x1b[5;9R";
        let mut replies = Vec::new();

        while !replies.windows(answer.len()).any(|window| window == answer) {
            assert!(
                Instant::now() < deadline,
                "the cursor report was not answered: {replies:?}"
            );

            std::thread::sleep(Duration::from_millis(20));

            for (shell, event) in sessions.drain_graphics() {
                if shell == id
                    && let GraphicsEvent::Reply(bytes) = event
                {
                    replies.extend_from_slice(&bytes);
                }
            }
        }
    }
}


