// Syntax highlighting of the file preview, off the UI thread.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ratatui::text::Text;

use crate::utils::highlight_text;

/// A request for the highlighting thread.
///
/// It carries the preview's text, so the thread may sit on one request while
/// the cursor has moved on: only the newest one is worth keeping.
struct Job {
    path: PathBuf,
    content: String,
}

/// What the thread produced for one file.
///
/// `None` means the file has nothing to highlight: syntect knows no language
/// for it, its content is too large, or the parser failed.
pub type HighlightDone = (PathBuf, Option<Text<'static>>);

/// The thread which highlights the preview's text.
///
/// The preview is read on the UI thread (one window of lines is cheap), but
/// turning those lines into coloured spans is not: syntect parses every line of
/// the window again for every cursor movement. One worker does it in the
/// background, so the UI thread draws the unhighlighted text at once and
/// replaces it when the answer arrives.
///
/// Between previews the worker blocks on its channel, so an idle hire spends
/// nothing on it. Requests and answers are not piled up either: the worker
/// keeps only the newest request (browsing a directory must not queue one
/// highlight per file) and the UI thread keeps only the newest answer.
#[derive(Default)]
pub struct HighlightPreview {
    requests: Option<Sender<Job>>,
    answers: Option<Receiver<HighlightDone>>,

    /// The path of the last request, until its answer is taken. While it is
    /// set, the UI polls sooner so the colours are painted as soon as they are
    /// ready.
    requested: Option<PathBuf>,
}

impl HighlightPreview {
    /// Start the highlighting thread. Without it the preview stays plain.
    pub fn start(&mut self) {
        let (request_tx, request_rx) = mpsc::channel::<Job>();
        let (answer_tx, answer_rx) = mpsc::channel::<HighlightDone>();

        thread::spawn(move || {
            while let Ok(mut job) = request_rx.recv() {
                // A request the cursor has already left is skipped: only the
                // newest one can still be shown.
                while let Ok(newer) = request_rx.try_recv() {
                    job = newer;
                }

                let text = highlight_text(&job.content, &job.path);

                // Sending fails once the App is gone, which is how the thread
                // ends when hire quits.
                if answer_tx.send((job.path, text)).is_err() {
                    break
                }
            }
        });

        self.requests = Some(request_tx);
        self.answers = Some(answer_rx);
    }

    /// Ask for the preview `content` of `path` to be highlighted.
    ///
    /// A request the thread has not started yet is replaced, since the cursor
    /// has moved on and its text would never be shown.
    pub fn request(&mut self, path: PathBuf, content: String) {
        let Some(requests) = self.requests.as_ref() else {
            return
        };

        let job = Job {
            path: path.clone(),
            content
        };

        if requests.send(job).is_ok() {
            self.requested = Some(path);
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

        // An answer for a file the cursor already left does not end the wait
        // for the current one.
        if newest.as_ref().is_some_and(|(path, _)| {
            Some(path) == self.requested.as_ref()
        }) {
            self.requested = None;
        }

        newest
    }
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
    /// path of the file, which is what tells the UI whether the answer still
    /// belongs to the selected file.
    #[test]
    fn test_highlight_is_answered_with_its_path() {
        let mut preview = HighlightPreview::default();
        preview.start();

        preview.request(
            PathBuf::from("/tmp/hire-highlight-test.rs"),
            String::from("fn main() {}\n")
        );

        let (path, text) = wait(&mut preview).unwrap();

        assert_eq!(path, PathBuf::from("/tmp/hire-highlight-test.rs"));

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
            String::from("plain\n")
        );

        let (_, text) = wait(&mut preview).unwrap();

        assert!(text.is_none());
        assert!(!preview.pending());
    }

    /// Without a worker (as in most tests) a request is a no-op: the preview
    /// stays plain and nothing is left pending.
    #[test]
    fn test_a_stopped_worker_ignores_requests() {
        let mut preview = HighlightPreview::default();

        preview.request(PathBuf::from("a.rs"), String::from("fn main() {}\n"));

        assert!(!preview.pending());
        assert!(preview.take().is_none());
    }
}
