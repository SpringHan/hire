// Shell sessions.
//
// A session is a PTY-backed shell plus its parsed screen. Sessions belong to a
// tab and survive being detached: the reader thread keeps consuming and
// parsing output while the file browser is shown.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, MutexGuard,
    },
};

use crate::{error::AppResult, key_event::TabId, utils::ShellId};

use super::pty::PtyProcess;

/// The size a session is spawned with; the first draw resizes it to the real
/// terminal area.
pub const DEFAULT_PTY_ROWS: u16 = 24;
pub const DEFAULT_PTY_COLS: u16 = 80;

/// How many scrolled-off lines are kept for every session.
pub const SCROLLBACK_ROWS: usize = 240;

pub struct ShellMeta {
    /// The basename of the shell program, e.g. `bash`.
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
                parser.set_size(rows, cols);
            }
        }

        self.pty.resize(rows, cols)
    }
}

pub struct ShellSessions {
    sessions: HashMap<ShellId, ShellSession>,
    next_id: u64,
    exit_tx: Sender<ShellId>,
    exit_rx: Receiver<ShellId>,
}

impl Default for ShellSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellSessions {
    pub fn new() -> Self {
        let (exit_tx, exit_rx) = mpsc::channel();

        Self {
            sessions: HashMap::new(),
            next_id: 0,
            exit_tx,
            exit_rx,
        }
    }

    /// Spawn a new shell and return its id.
    pub fn open(&mut self, tab: TabId, program: &str, cwd: &Path) -> AppResult<ShellId> {
        let id = ShellId(self.next_id);
        self.next_id += 1;

        let parser = vt100::Parser::new(
            DEFAULT_PTY_ROWS,
            DEFAULT_PTY_COLS,
            SCROLLBACK_ROWS,
        );
        let screen = Arc::new(Mutex::new(parser));

        let pty = PtyProcess::spawn(
            program,
            cwd,
            DEFAULT_PTY_ROWS,
            DEFAULT_PTY_COLS,
            screen.clone(),
            self.exit_tx.clone(),
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


