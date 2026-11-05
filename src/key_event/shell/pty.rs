// A PTY-backed shell process.
//
// The child gets a private terminal, so hire keeps the real one and can
// detach/attach the shell at will. Output is parsed by a reader thread into
// the shared `vt100` screen.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        mpsc::{Sender, SyncSender},
        Arc, Mutex,
    },
};

use anyhow::anyhow;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::{error::AppResult, utils::ShellId};

use super::graphics::{GraphicsEvent, GraphicsFilter, GraphicsSupport};

/// The graphics channel of a session's reader thread.
pub struct PtyGraphics {
    pub support: GraphicsSupport,
    pub events: SyncSender<(ShellId, GraphicsEvent)>,
}

pub struct PtyProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    /// Cell size in pixels; `(0, 0)` when it is unknown.
    cell: (u16, u16),
}

impl PtyProcess {
    /// Spawn `program` in a new PTY.
    ///
    /// The reader thread keeps parsing `program`'s output into `screen` until
    /// the slave side is closed, then sends the shell id through `exited`.
    /// Kitty image sequences are split off for `graphics` instead of being
    /// parsed as text.
    pub fn spawn(
        program: &str,
        cwd: &Path,
        size: PtySize,
        screen: Arc<Mutex<vt100::Parser>>,
        exited: Sender<ShellId>,
        graphics: PtyGraphics,
        id: ShellId,
    ) -> AppResult<Self>
    {
        let rows = size.rows.max(1);
        let cols = size.cols.max(1);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: size.pixel_width,
                pixel_height: size.pixel_height,
            })
            .map_err(|err| anyhow!("Cannot open a pty: {err}"))?;

        let mut command = CommandBuilder::new(program);
        command.cwd(cwd);

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| anyhow!("Cannot spawn `{program}`: {err}"))?;

        // The slave end must be closed in the parent, otherwise reads never
        // see EOF when the child exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| anyhow!("Cannot read from the pty: {err}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| anyhow!("Cannot write to the pty: {err}"))?;

        let cell = graphics.support.cell;

        std::thread::Builder::new()
            .name(format!("hire-shell-{}", id.0))
            .spawn(move || {
                let mut buf = [0u8; 8192];
                let mut filter = GraphicsFilter::new(graphics.support);
                let events = graphics.events;

                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut parser = screen
                                .lock()
                                .unwrap_or_else(|err| err.into_inner());
                            let (rows, cols) = parser.screen().size();

                            let mut emit = |event: GraphicsEvent| {
                                let _ = events.try_send((id, event));
                            };

                            filter.feed(
                                &buf[..n],
                                rows,
                                cols,
                                &mut *parser,
                                &mut emit,
                            );
                        },
                    }
                }

                let _ = exited.send(id);
            })?;

        Ok(Self {
            master: pair.master,
            writer,
            child,
            size: (rows, cols),
            cell,
        })
    }

    /// Forward terminal bytes to the shell.
    pub fn write(&mut self, bytes: &[u8]) -> AppResult<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;

        Ok(())
    }

    /// Resize the PTY; the kernel sends `SIGWINCH` to the child.
    pub fn resize(&mut self, rows: u16, cols: u16) -> AppResult<()> {
        let rows = rows.max(1);
        let cols = cols.max(1);

        if (rows, cols) == self.size {
            return Ok(())
        }

        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: cols.saturating_mul(self.cell.0),
                pixel_height: rows.saturating_mul(self.cell.1),
            })
            .map_err(|err| anyhow!("Cannot resize the pty: {err}"))?;

        self.size = (rows, cols);
        Ok(())
    }

    /// The pid of the process group which currently owns the terminal.
    ///
    /// The shell and everything it runs live on the slave side of the PTY, so
    /// the foreground job (`vim`, `htop`, …) is the process group the kernel
    /// reports for that terminal. `tcgetpgrp` on the master end queries the
    /// slave's terminal, not hire's own.
    pub fn foreground_pid(&self) -> Option<u32> {
        let fd = self.master.as_raw_fd()?;

        // SAFETY: `fd` is the open master end of the PTY owned by `self`.
        let pgrp = unsafe { libc::tcgetpgrp(fd) };

        if pgrp <= 0 {
            None
        } else {
            Some(pgrp as u32)
        }
    }

    /// Terminate the child and reap it.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        // Reap it so a closed session does not leave a zombie behind. SIGKILL
        // makes this immediate.
        let _ = self.child.wait();
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.kill();
    }
}
