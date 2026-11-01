// A PTY-backed shell process.
//
// The child gets a private terminal, so hire keeps the real one and can
// detach/attach the shell at will. Output is parsed by a reader thread into
// the shared `vt100` screen.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{mpsc::Sender, Arc, Mutex},
};

use anyhow::anyhow;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::{error::AppResult, utils::ShellId};

pub struct PtyProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
}

impl PtyProcess {
    /// Spawn `program` in a new PTY of `rows` x `cols`.
    ///
    /// The reader thread keeps parsing `program`'s output into `screen` until
    /// the slave side is closed, then sends the shell id through `exited`.
    pub fn spawn(
        program: &str,
        cwd: &Path,
        rows: u16,
        cols: u16,
        screen: Arc<Mutex<vt100::Parser>>,
        exited: Sender<ShellId>,
        id: ShellId,
    ) -> AppResult<Self>
    {
        let rows = rows.max(1);
        let cols = cols.max(1);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
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

        std::thread::Builder::new()
            .name(format!("hire-shell-{}", id.0))
            .spawn(move || {
                let mut buf = [0u8; 8192];

                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut parser) = screen.lock() {
                                parser.process(&buf[..n]);
                            }
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
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| anyhow!("Cannot resize the pty: {err}"))?;

        self.size = (rows, cols);
        Ok(())
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
