// Syntax highlighting of the file preview, off the UI thread.

use std::{
    fs::File,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use ratatui::text::Text;

use crate::utils::{highlight_text, read_prev_window};

/// How many lines before the window are highlighted as context.
///
/// Syntect keeps its parser state from line to line (a block comment or a
/// string can span many lines). The lines before the window seed that state;
/// they are thrown away afterwards, so only the window is painted.
const CONTEXT_LINES: usize = 32;

/// A request for the highlighting thread.
///
/// It carries a window of the preview, so the thread may sit on one request
/// while the user has scrolled on: only the newest window is worth keeping.
struct Job {
    path: PathBuf,
    start_line: usize,
    start_byte: u64,
    content: String,
    generation: u64,
}

/// What the thread produced for one window.
///
/// The generation is the preview's window generation at the time of the
/// request; `None` means the window has nothing to highlight (syntect knows no
/// language for it, or the parser failed).
pub type HighlightDone = (PathBuf, u64, Option<Text<'static>>);

/// The thread which highlights the preview's window.
///
/// Only the lines the terminal can show (plus a margin and a context prefix) are
/// read and highlighted, so the cost is bounded by the pane, not by the file.
/// One worker does it in the background, so the UI thread draws the
/// unhighlighted text at once and replaces it when the answer arrives.
///
/// Between windows the worker blocks on its channel, so an idle hire spends
/// nothing on it. Requests and answers are not piled up either: the worker
/// keeps only the newest request (browsing a directory must not queue one
/// highlight per file) and the UI thread keeps only the newest answer.
#[derive(Default)]
pub struct HighlightPreview {
    requests: Option<Sender<Job>>,
    answers: Option<Receiver<HighlightDone>>,

    /// The window of the last request, until its answer is taken. While it is
    /// set, the UI polls sooner so the colours are painted as soon as they are
    /// ready. The generation tells an answer for an older window from the one
    /// the UI is waiting for.
    requested: Option<(PathBuf, u64)>,
}

impl HighlightPreview {
    /// Start the highlighting thread. Without it the preview stays plain.
    pub fn start(&mut self) {
        let (request_tx, request_rx) = mpsc::channel::<Job>();
        let (answer_tx, answer_rx) = mpsc::channel::<HighlightDone>();

        thread::spawn(move || {
            while let Ok(mut job) = request_rx.recv() {
                // A window the user has already left is skipped: only the
                // newest one can still be shown.
                while let Ok(newer) = request_rx.try_recv() {
                    job = newer;
                }

                let text = highlight_job(&job);

                // Sending fails once the App is gone, which is how the thread
                // ends when hire quits.
                if answer_tx.send((job.path, job.generation, text)).is_err() {
                    break
                }
            }
        });

        self.requests = Some(request_tx);
        self.answers = Some(answer_rx);
    }

    /// Ask for the preview window `content` of `path` to be highlighted.
    ///
    /// `start_line` and `start_byte` locate the window in the file, so the
    /// worker can read the lines before it as syntect context.
    pub fn request(
        &mut self,
        path: PathBuf,
        start_line: usize,
        start_byte: u64,
        content: String,
        generation: u64,
    ) {
        let Some(requests) = self.requests.as_ref() else {
            return
        };

        let job = Job {
            path: path.clone(),
            start_line,
            start_byte,
            content,
            generation,
        };

        if requests.send(job).is_ok() {
            self.requested = Some((path, generation));
        }
    }

    /// Whether the answer to the last request is still on its way.
    pub fn pending(&self) -> bool {
        self.requested.is_some()
    }

    /// The newest finished highlight, if the thread produced one since the last
    /// call.
    pub fn take(&mut self) -> Option<HighlightDone> {
        let answers = self.answers.as_ref()?;
        let mut newest = None;

        loop {
            match answers.try_recv() {
                Ok(answer) => newest = Some(answer),
                Err(mpsc::TryRecvError::Empty) => break,
                // The thread is gone: no answer will ever arrive.
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.requested = None;
                    break
                },
            }
        }

        // An answer for another file, or for a window the UI has already
        // scrolled away from, does not end the wait for the current one.
        if newest.as_ref().is_some_and(|(path, generation, _)| {
            self.requested
                .as_ref()
                .is_some_and(|(requested, expected)| {
                    requested == path && expected == generation
                })
        }) {
            self.requested = None;
        }

        newest
    }
}

/// Highlight the window of `job`, with the lines before it as context.
fn highlight_job(job: &Job) -> Option<Text<'static>> {
    let mut content = String::new();
    let mut context_lines = 0;

    if let Ok(file) = File::open(&job.path) &&
        let Ok(prev) = read_prev_window(
            &file,
            job.start_byte,
            job.start_line,
            CONTEXT_LINES
        )
    {
        context_lines = prev.lines.len();

        for line in &prev.lines {
            content.push_str(line);
            content.push('\n');
        }
    }

    content.push_str(&job.content);

    let mut text = highlight_text(&content, &job.path)?;

    // Only the window itself is painted; the context was there for the parser.
    text.lines.drain(..context_lines.min(text.lines.len()));

    Some(text)
}

#[cfg(test)]
mod test {
    use std::time::{Duration, Instant};

    use super::*;

    /// Wait for the answer to the last request, like the main loop does.
    fn wait(preview: &mut HighlightPreview) -> Option<HighlightDone> {
        let deadline = Instant::now() + Duration::from_secs(5);

        while preview.pending() && Instant::now() < deadline {
            if let Some(answer) = preview.take() {
                return Some(answer)
            }

            thread::sleep(Duration::from_millis(1));
        }

        preview.take()
    }

    /// The thread highlights the text it was given and reports it back with the
    /// path and the window generation, which is what tells the UI whether the
    /// answer still belongs to the current window.
    #[test]
    fn test_highlight_is_answered_with_its_path() {
        let mut preview = HighlightPreview::default();
        preview.start();

        preview.request(
            PathBuf::from("/tmp/hire-highlight-test.rs"),
            0,
            0,
            String::from("fn main() {}\n"),
            7
        );

        let (path, generation, text) = wait(&mut preview).unwrap();

        assert_eq!(path, PathBuf::from("/tmp/hire-highlight-test.rs"));
        assert_eq!(generation, 7);

        let text = text.unwrap();
        let styles: Vec<_> = text.lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.style)
            .collect();
        assert!(styles.iter().any(|style| style.fg.is_some()));
    }

    /// A file with nothing to highlight is answered with `None`, so the UI
    /// stops waiting (and keeps the plain text) instead of polling forever.
    #[test]
    fn test_nothing_to_highlight_is_answered_too() {
        let mut preview = HighlightPreview::default();
        preview.start();

        preview.request(
            PathBuf::from("/tmp/hire-highlight-test.txt"),
            0,
            0,
            String::from("plain\n"),
            1
        );

        let (_, _, text) = wait(&mut preview).unwrap();

        assert!(text.is_none());
        assert!(!preview.pending());
    }

    /// Without a worker (as in most tests) a request is a no-op: the preview
    /// stays plain and nothing is left pending.
    #[test]
    fn test_a_stopped_worker_ignores_requests() {
        let mut preview = HighlightPreview::default();

        preview.request(
            PathBuf::from("a.rs"),
            0,
            0,
            String::from("fn main() {}\n"),
            1
        );

        assert!(!preview.pending());
        assert!(preview.take().is_none());
    }
}
