// Text preview: scrolling and searching.

use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use ratatui::text::Line;

use crate::{
    error::AppResult,
    rt_error,
    utils::{get_window_height, get_window_width, search_indexes, FileContent},
};

use super::App;

/// A finished preview search.
pub struct PreviewMatches {
    /// The search this answer belongs to.
    pub generation: u64,

    /// The indices of the lines which contain a match, in ascending order.
    pub lines: Vec<usize>,
}

/// A request to the preview search thread.
struct PreviewSearchRequest {
    generation: u64,
    query: String,
    lines: Vec<String>,
}

/// Scrolling and search state of the text preview.
///
/// The preview shows every line `utils::read_text_prefix` read (the whole file
/// up to `MAX_PREVIEW_BYTES`); this state decides which part of them is on
/// screen and which lines match the search.
#[derive(Default)]
pub struct PreviewState {
    /// The index of the first line shown in the preview.
    offset: usize,

    /// The first terminal column shown of every preview line.
    column: usize,

    /// The query of the active search.
    query: Option<String>,

    /// The lines which contain a match, in ascending order.
    matched_lines: Vec<usize>,

    /// The index in `matched_lines` the view last jumped to.
    current: Option<usize>,

    /// The search the next answer must belong to. Every new search (and every
    /// new preview) bumps it, so a late answer can never move the view of
    /// another search or another file.
    generation: u64,

    /// The channel to the search thread. `None` when the thread was not
    /// started: the preview still scrolls, the search is just unavailable.
    sender: Option<Sender<PreviewSearchRequest>>,
}

impl PreviewState {
    /// The index of the first line shown in the preview.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The first terminal column shown of every preview line.
    pub fn column(&self) -> usize {
        self.column
    }

    /// Whether a preview search is active (`n`/`N` scroll its matches).
    pub fn is_searching(&self) -> bool {
        self.query.is_some()
    }

    /// The character ranges of every occurrence of the query in `line`.
    ///
    /// The ranges are what the renderer reverses. They are in characters, not
    /// bytes, because the renderer walks the spans of the line character by
    /// character.
    pub fn match_ranges(&self, line: &Line<'_>) -> Vec<(usize, usize)> {
        let Some(query) = self.query.as_deref() else {
            return Vec::new()
        };

        let text: String = line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        match_ranges(&text, query)
    }

    /// Forget the scroll offset, the column and the search.
    pub fn reset(&mut self) {
        self.offset = 0;
        self.column = 0;
        self.clear_search();
    }

    /// Drop the query, its matches and any answer still in flight.
    pub fn clear_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.query = None;
        self.matched_lines.clear();
        self.current = None;
    }

    /// Start a new search; the answer of every older one becomes stale.
    fn begin_search(&mut self, query: String) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.query = Some(query);
        self.matched_lines.clear();
        self.current = None;

        self.generation
    }

    fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    fn set_column(&mut self, column: usize) {
        self.column = column;
    }
}

impl<'a> App<'a> {
    /// Start the thread which searches the preview, and return the channel its
    /// answers arrive on.
    ///
    /// The worker is the same shape as the file searcher's: it blocks on its
    /// channel between searches, so an idle hire spends nothing on it, and the
    /// matching rule is the shared `utils::search_indexes`.
    pub fn init_preview_search_channel(&mut self) -> Receiver<PreviewMatches> {
        let (update_tx, update_rx) = mpsc::channel::<PreviewMatches>();
        let (calc_tx, calc_rx) = mpsc::channel::<PreviewSearchRequest>();

        thread::spawn(move || {
            while let Ok(request) = calc_rx.recv() {
                let lines = search_indexes(
                    &request.query,
                    request.lines.iter().map(|line| line.as_str()),
                    false
                );

                let matches = PreviewMatches {
                    generation: request.generation,
                    lines,
                };

                if update_tx.send(matches).is_err() {
                    break
                }
            }
        });

        self.preview.sender = Some(calc_tx);

        update_rx
    }

    /// Drop the preview and the scroll/search state which belongs to it.
    pub fn reset_preview(&mut self) {
        self.file_content.reset();
        self.preview.reset();
    }

    /// Search `query` in the text preview.
    ///
    /// The lines are handed to the search thread, so the UI never waits for a
    /// long file; `apply_preview_search` moves the view when the answer
    /// arrives. An empty query clears the previous search (and its highlight).
    pub fn preview_search(&mut self, query: String) -> AppResult<()> {
        if query.is_empty() {
            self.preview.clear_search();
            return Ok(())
        }

        let Some(sender) = self.preview.sender.clone() else {
            rt_error!("The preview search thread is not running")
        };

        let FileContent::Text(ref content) = self.file_content else {
            rt_error!("There is no text preview to search in")
        };

        let lines: Vec<String> = content.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();

        // Like a `/` search, the command is kept in the history.
        self.command_history.push(format!("?{query}"));

        let generation = self.preview.begin_search(query.clone());

        if sender
            .send(PreviewSearchRequest {
                generation,
                query,
                lines,
            })
            .is_err()
        {
            self.preview.clear_search();
            rt_error!("The preview search thread is gone")
        }

        Ok(())
    }

    /// Apply a finished preview search and move the view to its first match.
    pub fn apply_preview_search(&mut self, matches: PreviewMatches) {
        // A newer search (or another file's preview) replaced this one while
        // the thread was working.
        if matches.generation != self.preview.generation {
            return
        }

        self.preview.matched_lines = matches.lines;

        if self.preview.matched_lines.is_empty() {
            self.preview.current = None;
            return
        }

        // Start from what is on screen, so scrolling down and searching again
        // finds the next match instead of jumping back to the top.
        let from = self.preview.offset;
        let idx = self.preview.matched_lines
            .iter()
            .position(|line| *line >= from)
            .unwrap_or(0);

        self.preview.current = Some(idx);
        let line = self.preview.matched_lines[idx];
        self.preview_jump_to(line);
    }

    /// Scroll the text preview by one line.
    pub fn preview_scroll_line(&mut self, down: bool) {
        self.scroll_preview(if down { 1 } else { -1 });
    }

    /// Scroll the text preview by one window height.
    pub fn preview_scroll_page(&mut self, down: bool) {
        let page = (get_window_height() as usize).max(1) as isize;

        self.scroll_preview(if down { page } else { -page });
    }

    /// Scroll the text preview by one column.
    ///
    /// The right end of the widest line stays at the right edge of the pane, so
    /// the preview never scrolls off the text into an empty screen.
    pub fn preview_scroll_column(&mut self, right: bool) {
        let Some(longest) = preview_line_width(self) else {
            return
        };

        let column = if right {
            self.preview.column.saturating_add(5)
        } else {
            self.preview.column.saturating_sub(5)
        };

        self.preview.set_column(column.min(preview_max_column(longest)));
    }

    /// Jump to the next/previous match of the preview search.
    ///
    /// The matches wrap around, so `n` at the last one continues at the first.
    pub fn preview_move_candidate(&mut self, next: bool) {
        let count = self.preview.matched_lines.len();
        if count == 0 {
            return
        }

        let target = match self.preview.current {
            Some(current) if next => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if next => 0,
            None => count - 1,
        };

        self.preview.current = Some(target);
        let line = self.preview.matched_lines[target];
        self.preview_jump_to(line);
    }

    /// Whether `n`/`N` navigate the preview instead of the file list.
    ///
    /// A preview search which found nothing still owns the keys: the user
    /// searched the preview, so `n`/`N` must not quietly move through the file
    /// list's matches. Selecting another file (or another preview) resets the
    /// preview, which gives the keys back.
    pub fn preview_search_active(&self) -> bool {
        self.preview.is_searching() &&
            matches!(self.file_content, FileContent::Text(_))
    }

    /// Scroll the text preview by `delta` lines, clamped to what it holds.
    fn scroll_preview(&mut self, delta: isize) {
        let Some(total) = preview_line_count(self) else {
            return
        };

        let offset = self.preview.offset.saturating_add_signed(delta);

        self.preview.set_offset(offset.min(preview_max_offset(total)));
    }

    /// Put `line` in the middle of the preview when possible.
    ///
    /// The jump also returns to the first column: a horizontal scroll which
    /// was left in place would hide the match at the beginning of the line.
    fn preview_jump_to(&mut self, line: usize) {
        let Some(total) = preview_line_count(self) else {
            return
        };

        let page = (get_window_height() as usize).max(1);
        let offset = line.saturating_sub(page / 2);

        self.preview.set_offset(offset.min(preview_max_offset(total)));
        self.preview.set_column(0);
    }
}

/// The number of lines the text preview holds, when it shows text.
fn preview_line_count(app: &App) -> Option<usize> {
    match &app.file_content {
        FileContent::Text(text) => Some(text.lines.len()),
        _ => None,
    }
}

/// The largest offset the text preview can be scrolled to.
///
/// The last line stays at the bottom of the pane, so the view does not scroll
/// past the end of the file leaving an empty screen.
fn preview_max_offset(total: usize) -> usize {
    let page = (get_window_height() as usize).max(1);

    total.saturating_sub(page).min(total.saturating_sub(1))
}

/// The width of the widest line of the text preview, in terminal columns.
fn preview_line_width(app: &App) -> Option<usize> {
    let FileContent::Text(ref text) = app.file_content else {
        return None
    };

    Some(text.lines.iter().map(|line| line.width()).max().unwrap_or(0))
}

/// The largest column the text preview can be scrolled to.
///
/// The end of the widest line stays at the right edge of the pane. A line which
/// fits the pane cannot be scrolled at all; the shift is also kept in `u16`,
/// which is what the renderer's scroll offset uses.
fn preview_max_column(longest: usize) -> usize {
    let width = (get_window_width() as usize).max(1);

    longest.saturating_sub(width).min(u16::MAX as usize)
}

/// The character ranges of every occurrence of `query` in `text`.
///
/// The comparison is case-insensitive, like the file search. Lowercasing a
/// character can expand it (`İ` becomes two characters), so the positions of
/// the lowercased text are mapped back to the characters of `text` instead of
/// being used as they are. The matches do not overlap: `aa` occurs once in
/// `aaa`.
fn match_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    if query.is_empty() {
        return ranges
    }

    // `map` holds the index of the original character for every character of
    // `lowered`.
    let mut lowered: Vec<char> = Vec::new();
    let mut map: Vec<usize> = Vec::new();

    for (idx, ch) in text.chars().enumerate() {
        for lower in ch.to_lowercase() {
            lowered.push(lower);
            map.push(idx);
        }
    }

    let query: Vec<char> = query.to_lowercase().chars().collect();

    let mut start = 0;
    while start + query.len() <= lowered.len() {
        if lowered[start..start + query.len()] == query[..] {
            ranges.push((map[start], map[start + query.len() - 1] + 1));
            start += query.len();
            continue
        }

        start += 1;
    }

    ranges
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Mutex,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use ratatui::text::Span;

    use crate::utils::{
        update_window_height,
        update_window_width,
        FileContent,
    };

    use super::*;

    /// `WINDOW_HEIGHT` is process-wide, so the preview tests take this lock
    /// while they size a page and restore the height afterwards.
    static WINDOW_HEIGHT_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-preview-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Take the window-size lock and set the pane size the scroll clamps use;
    /// the test sets both back before the guard is dropped.
    fn window_size(
        height: u16,
        width: u16,
    ) -> std::sync::MutexGuard<'static, ()> {
        let guard = WINDOW_HEIGHT_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());

        update_window_height(height);
        update_window_width(width);

        guard
    }

    fn write_lines(dir: &Path, lines: &[String]) {
        fs::write(dir.join("a.txt"), lines.join("\n")).unwrap();
    }

    fn numbered_lines(count: usize) -> Vec<String> {
        (0..count).map(|idx| format!("line {idx}")).collect()
    }

    /// Run the search and apply its answer, like the main loop does.
    fn search(app: &mut App, rx: &Receiver<PreviewMatches>, query: &str) {
        app.preview_search(String::from(query)).unwrap();

        let answer = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the preview search thread answered");

        app.apply_preview_search(answer);
    }

    /// The preview shows the whole read file, so a line, a page and the end of
    /// the text are reachable and clamped.
    #[test]
    fn test_preview_scrolls_line_and_page() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("scroll");
        write_lines(&dir, &numbered_lines(50));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        app.preview_scroll_line(true);
        assert_eq!(app.preview.offset(), 1);

        app.preview_scroll_line(false);
        assert_eq!(app.preview.offset(), 0);

        // The first line cannot be scrolled away at the top.
        app.preview_scroll_line(false);
        assert_eq!(app.preview.offset(), 0);

        app.preview_scroll_page(true);
        assert_eq!(app.preview.offset(), 10);

        app.preview_scroll_page(false);
        assert_eq!(app.preview.offset(), 0);

        // The last page keeps the last line at the bottom of the pane.
        for _ in 0..10 {
            app.preview_scroll_page(true);
        }
        assert_eq!(app.preview.offset(), 40);

        // A directory has no text preview: scrolling does nothing.
        app.preview.reset();
        app.file_content = FileContent::None;
        app.preview_scroll_line(true);
        assert_eq!(app.preview.offset(), 0);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The column offset moves over the widest line and stops at the right
    /// edge of the pane; a line which fits cannot be scrolled.
    #[test]
    fn test_preview_scrolls_columns() {
        let _guard = window_size(10, 10);

        let dir = temp_dir("columns");
        write_lines(&dir, &[
            String::from("0123456789ABCDEFGHIJ"),
            String::from("short"),
        ]);

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        app.preview_scroll_column(true);
        assert_eq!(app.preview.column(), 1);

        app.preview_scroll_column(false);
        assert_eq!(app.preview.column(), 0);

        // The first column stays at the left edge.
        app.preview_scroll_column(false);
        assert_eq!(app.preview.column(), 0);

        // The 20-column line in a 10-column pane: the last visible column is 10.
        for _ in 0..50 {
            app.preview_scroll_column(true);
        }
        assert_eq!(app.preview.column(), 10);

        // A file whose lines fit the pane has nothing to scroll.
        write_lines(&dir, &[String::from("short")]);
        app.set_file_content().unwrap();
        app.preview_scroll_column(true);
        assert_eq!(app.preview.column(), 0);

        // A new preview starts at the first column as well.
        write_lines(&dir, &[String::from("0123456789ABCDEFGHIJ")]);
        app.set_file_content().unwrap();
        app.preview_scroll_column(true);
        assert_eq!(app.preview.column(), 1);

        // A search jump returns to the first column, so the match is not left
        // hidden behind a horizontal scroll.
        let rx = app.init_preview_search_channel();
        search(&mut app, &rx, "ABCD");
        assert_eq!(app.preview.matched_lines, vec![0]);
        assert_eq!(app.preview.column(), 0);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The search jumps to a match, `n`/`N` walk the matches (wrapping around)
    /// and a new search replaces the old matches.
    #[test]
    fn test_preview_search_and_move_candidate() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("search");
        let mut lines = numbered_lines(50);
        lines[3] = String::from("Needle here");
        lines[30] = String::from("needle here");
        write_lines(&dir, &lines);

        let mut app = App::default();
        let rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        // The search is case-insensitive and keeps every matching line.
        search(&mut app, &rx, "needle");
        assert_eq!(app.preview.matched_lines, vec![3, 30]);
        assert_eq!(app.preview.current, Some(0));

        // While the preview is text, `n`/`N` belong to its search.
        assert!(app.preview_search_active());

        // The match is centred: line 3 is shown at the top of a ten-line pane.
        assert_eq!(app.preview.offset(), 0);

        app.preview_move_candidate(true);
        assert_eq!(app.preview.current, Some(1));
        assert_eq!(app.preview.offset(), 25);

        // `n` at the last match wraps around to the first one.
        app.preview_move_candidate(true);
        assert_eq!(app.preview.current, Some(0));
        assert_eq!(app.preview.offset(), 0);

        // `N` at the first match wraps around to the last one.
        app.preview_move_candidate(false);
        assert_eq!(app.preview.current, Some(1));
        assert_eq!(app.preview.offset(), 25);

        // A search without a match keeps the view where it is.
        search(&mut app, &rx, "nothing-matches");
        assert!(app.preview.matched_lines.is_empty());
        assert_eq!(app.preview.current, None);
        assert_eq!(app.preview.offset(), 25);

        // An empty query clears the search, so `n`/`N` leave the preview.
        app.preview_search(String::new()).unwrap();
        assert!(!app.preview.is_searching());
        assert!(!app.preview_search_active());
        assert!(app.preview.matched_lines.is_empty());

        // A preview which is not text (an image, a directory listing) keeps
        // `n`/`N` for the file list even when a query is still set.
        app.preview.clear_search();
        app.preview.query = Some(String::from("needle"));
        app.file_content = FileContent::None;
        assert!(!app.preview_search_active());

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A directory has no text to search: the command line shows an error
    /// instead of starting a search.
    #[test]
    fn test_preview_search_without_text_is_an_error() {
        let dir = temp_dir("no-text");
        fs::create_dir(dir.join("folder")).unwrap();

        let mut app = App::default();
        let _rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        // Directories sort before files, so `folder` is selected.
        assert!(app.file_content == FileContent::None);
        assert!(app.preview_search(String::from("needle")).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    /// An answer for a search which was replaced (or for another preview) must
    /// never move the view.
    #[test]
    fn test_stale_preview_search_is_dropped() {
        let dir = temp_dir("stale");
        write_lines(&dir, &numbered_lines(5));

        let mut app = App::default();
        let rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        app.preview_search(String::from("line")).unwrap();
        let stale = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the preview search thread answered");
        assert_eq!(stale.generation, app.preview.generation);

        // The preview changed (another file or the same one re-read): the
        // answer belongs to the search of the old preview.
        app.preview.reset();
        app.apply_preview_search(stale);

        assert!(app.preview.matched_lines.is_empty());
        assert_eq!(app.preview.current, None);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// The ranges are in characters across the spans of the line, so a match
    /// can cross a syntax-highlighting span boundary, and the query is matched
    /// case-insensitively.
    #[test]
    fn test_preview_match_ranges() {
        let state = PreviewState {
            query: Some(String::from("needle")),
            ..Default::default()
        };

        let line = Line::from(vec![
            Span::raw("a nee"),
            Span::raw("dle b NEEDLE"),
        ]);
        assert_eq!(state.match_ranges(&line), vec![(2, 8), (11, 17)]);

        // No query: nothing is highlighted.
        let state = PreviewState::default();
        assert!(state.match_ranges(&line).is_empty());

        // Multi-byte text: the ranges count characters.
        assert_eq!(
            match_ranges("中文needle中文", "needle"),
            vec![(2, 8)]
        );

        // A character whose lowercase form is two characters is matched as a
        // whole; `aa` does not overlap in `aaa`.
        let lowered = 'İ'.to_lowercase().collect::<String>();
        assert_eq!(match_ranges("İx", &format!("{lowered}x")), vec![(0, 2)]);
        assert_eq!(match_ranges("aaa", "aa"), vec![(0, 2)]);
        assert!(match_ranges("abc", "").is_empty());
    }
}
