// Text preview: windowed file reading, scrolling and searching.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use ratatui::text::Line;

use crate::{
    error::AppResult,
    rt_error,
    utils::{
        get_window_height,
        get_window_width,
        plain_text,
        read_prev_window,
        read_window_skipping,
        search_indexes,
        FileContent,
        PreviewWindow,
        read_window,
    },
};

use super::{highlight::highlight_window, App};

/// How many matches a preview search keeps.
///
/// A file whose every line matches would otherwise grow the result without a
/// bound; the first matches are the ones the user walks through anyway.
const MAX_SEARCH_MATCHES: usize = 100_000;

/// How many lines the search reads from the file at a time.
const SEARCH_BATCH_LINES: usize = 256;

/// How many pages the preview keeps loaded.
///
/// One page above the view and two below it after a scroll: a page down (or a
/// page of line scrolling) stays inside the window, so it does not reload and
/// flash.
const PREVIEW_WINDOW_PAGES: usize = 4;

/// Which page of the window the view starts on after a scroll.
const PREVIEW_WINDOW_ABOVE_PAGES: usize = 1;

/// The largest window which is highlighted before it is drawn.
///
/// Highlighting happens on the UI thread for a reload, so the plain text is
/// never painted; a window of very long lines (a minified file) would block
/// that thread in syntect, so those rare windows keep the asynchronous path
/// (and its plain first frame) instead.
const MAX_SYNC_HIGHLIGHT_BYTES: usize = 128 * 1024;

/// One line of the preview which contains a search match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewMatch {
    /// The absolute index of the line.
    pub line: usize,

    /// The byte offset the line starts at.
    pub byte: u64,
}

/// A finished preview search.
pub struct PreviewMatches {
    /// The search this answer belongs to.
    pub generation: u64,

    /// The matches, in ascending line order.
    pub matches: Vec<PreviewMatch>,
}

/// A request to the preview search thread.
struct PreviewSearchRequest {
    generation: u64,

    /// The value of [`PreviewState::ticket`] this search belongs to.
    ticket: u64,

    query: String,

    path: PathBuf,
}

/// Scrolling and search state of the text preview.
///
/// The preview holds only a window of the file (the visible page plus a
/// margin), so opening or scrolling a huge file costs what the terminal shows,
/// not the size of the file. This state decides which part of the window is on
/// screen and where the next window starts.
#[derive(Default)]
pub struct PreviewState {
    /// The absolute index of the first line shown in the preview.
    offset: usize,

    /// The first terminal column shown of every preview line.
    column: usize,

    /// The absolute line index of the first line of the loaded window.
    window_start: usize,

    /// The byte offset `window_start` starts at.
    window_start_byte: u64,

    /// The byte offset every loaded line starts at.
    window_starts: Vec<u64>,

    /// The byte offset just after the last loaded line.
    window_end_byte: u64,

    /// Whether the loaded window reaches the end of the file.
    window_eof: bool,

    /// Bumped when a window is installed, so a highlight answer for an older
    /// window is never painted over the current one.
    window_generation: u64,

    /// The file the preview belongs to; `None` when there is no text preview
    /// (an image, a directory listing, a message).
    path: Option<PathBuf>,

    /// The query of the active search.
    query: Option<String>,

    /// The lines which contain a match, in ascending order.
    matches: Vec<PreviewMatch>,

    /// The index in `matches` the view last jumped to.
    current: Option<usize>,

    /// The search the next answer must belong to. Every new search (and every
    /// new preview) bumps it, so a late answer can never move the view of
    /// another search or another file.
    generation: u64,

    /// The channel to the search thread. `None` when the thread was not
    /// started: the preview still scrolls, the search is just unavailable.
    sender: Option<Sender<PreviewSearchRequest>>,

    /// The value the running search was started with. A new search bumps it,
    /// which tells the worker to stop scanning the old file.
    ticket: Arc<AtomicU64>,
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

    /// The absolute line index of the first line of the loaded window.
    pub fn window_start(&self) -> usize {
        self.window_start
    }

    /// The generation of the loaded window; a highlight answer carries it.
    #[cfg(test)]
    pub fn window_generation(&self) -> u64 {
        self.window_generation
    }

    /// Whether `path` and `generation` describe the window on screen.
    pub fn is_current_window(&self, path: &std::path::Path, generation: u64) -> bool {
        self.path.as_deref() == Some(path) && generation == self.window_generation
    }

    /// The line of the match `n`/`N` last moved to.
    ///
    /// The renderer underlines this match, so the occurrence the user selected
    /// stands out from the other matches of the same query.
    pub fn current_line(&self) -> Option<usize> {
        self.current
            .and_then(|idx| self.matches.get(idx))
            .map(|found| found.line)
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
        self.window_start = 0;
        self.window_start_byte = 0;
        self.window_starts.clear();
        self.window_end_byte = 0;
        self.window_eof = false;
        self.path = None;
        self.clear_search();
    }

    /// Drop the query, its matches and any answer still in flight.
    pub fn clear_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.query = None;
        self.matches.clear();
        self.current = None;
        self.ticket.fetch_add(1, Ordering::SeqCst);
    }

    /// Start a new search; the answer of every older one becomes stale.
    fn begin_search(&mut self, query: String) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.query = Some(query);
        self.matches.clear();
        self.current = None;

        self.generation
    }

    /// Make `window` the loaded window, and return the generation a highlight
    /// of it must carry.
    fn install_window(
        &mut self,
        window: &PreviewWindow,
        start_byte: u64,
    ) -> u64 {
        self.window_start = window.start_line;
        self.window_start_byte = window.starts
            .first()
            .copied()
            .unwrap_or(start_byte);
        self.window_starts.clone_from(&window.starts);
        self.window_end_byte = window.end_byte;
        self.window_eof = window.eof;

        self.window_generation = self.window_generation.wrapping_add(1);

        self.window_generation
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
    /// The worker scans the file itself, a batch of lines at a time, so a huge
    /// file never has to be loaded to be searched. It blocks on its channel
    /// between searches, so an idle hire spends nothing on it.
    pub fn init_preview_search_channel(&mut self) -> Receiver<PreviewMatches> {
        let (update_tx, update_rx) = mpsc::channel::<PreviewMatches>();
        let (calc_tx, calc_rx) = mpsc::channel::<PreviewSearchRequest>();
        let ticket = self.preview.ticket.clone();

        thread::spawn(move || {
            while let Ok(request) = calc_rx.recv() {
                let matches = search_preview_file(&request, &ticket);

                if update_tx.send(matches).is_err() {
                    break
                }
            }
        });

        self.preview.sender = Some(calc_tx);

        update_rx
    }

    /// Start the text preview of `path` with its first window.
    pub fn start_preview(&mut self, path: PathBuf) -> anyhow::Result<()> {
        self.preview.path = Some(path);
        self.load_preview_window_skipping(0, 0, 0, false)
    }

    /// Drop the preview and the scroll/search state which belongs to it.
    pub fn reset_preview(&mut self) {
        self.file_content.reset();
        self.preview.reset();
    }

    /// Search `query` in the text preview.
    ///
    /// The worker scans the file in the background, so the UI never waits for
    /// a long file; `apply_preview_search` moves the view when the answer
    /// arrives. An empty query clears the previous search (and its highlight).
    pub fn preview_search(&mut self, query: String) -> AppResult<()> {
        if query.is_empty() {
            self.preview.clear_search();
            return Ok(())
        }

        let Some(sender) = self.preview.sender.clone() else {
            rt_error!("The preview search thread is not running")
        };

        let Some(path) = self.preview.path.clone() else {
            rt_error!("There is no text preview to search in")
        };

        // Like a `/` search, the command is kept in the history.
        self.command_history.push(format!("?{query}"));

        let generation = self.preview.begin_search(query.clone());
        let ticket = self.preview.ticket
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);

        if sender
            .send(PreviewSearchRequest {
                generation,
                ticket,
                query,
                path,
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

        self.preview.matches = matches.matches;

        if self.preview.matches.is_empty() {
            self.preview.current = None;
            return
        }

        // Start from what is on screen, so scrolling down and searching again
        // finds the next match instead of jumping back to the top.
        let from = self.preview.offset;
        let idx = self.preview.matches
            .iter()
            .position(|found| found.line >= from)
            .unwrap_or(0);

        self.preview.current = Some(idx);
        let target = self.preview.matches[idx];

        if let Err(err) = self.preview_jump_to(target) {
            self.app_error.add_error(err);
        }
    }

    /// Scroll the text preview by one line.
    pub fn preview_scroll_line(&mut self, down: bool) {
        self.scroll_preview(if down { 1 } else { -1 });
    }

    /// Scroll the text preview by one window height.
    pub fn preview_scroll_page(&mut self, down: bool) {
        let page = preview_page() as isize;

        self.scroll_preview(if down { page } else { -page });
    }

    /// Scroll the text preview by five columns.
    ///
    /// The right end of the widest visible line stays at the right edge of the
    /// pane, so the preview never scrolls off the text into an empty screen.
    pub fn preview_scroll_column(&mut self, right: bool) {
        let Some(longest) = preview_visible_width(self) else {
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
        let count = self.preview.matches.len();
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
        let found = self.preview.matches[target];

        if let Err(err) = self.preview_jump_to(found) {
            self.app_error.add_error(err);
        }
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

    /// Make sure the loaded window covers what the terminal shows.
    ///
    /// The terminal can be resized while the preview is open: a taller pane
    /// needs more lines than the window was read with, and a narrower one needs
    /// the horizontal scroll clamped again. The window is reloaded around the
    /// current offset when it no longer covers the page.
    pub fn ensure_preview_window(&mut self, height: u16) {
        if self.preview.path.is_none() ||
            !matches!(self.file_content, FileContent::Text(_))
        {
            return
        }

        let page = (height as usize).max(1);

        // The last page is clamped like a scroll is: a taller terminal on the
        // last page pulls the view up instead of leaving blank rows below the
        // end of the file.
        let mut target = self.preview.offset;

        if self.preview.window_eof {
            let total = self.preview.window_start +
                self.preview_window_len();

            target = if total < page { 0 } else { target.min(total - page) };
        }

        if (target != self.preview.offset ||
            !self.preview_window_covers(target, page)) &&
            let Err(err) = self.show_preview_line(target)
        {
            self.app_error.add_error(err);
        }

        self.clamp_preview_column();
    }

    /// Whether the loaded window shows a whole page from `target`.
    fn preview_window_covers(&self, target: usize, page: usize) -> bool {
        let start = self.preview.window_start;
        let len = self.preview_window_len();

        if target < start {
            return false
        }

        if len == 0 {
            return true
        }

        if target + page <= start + len {
            return true
        }

        // The page runs past the end of the file: the last page is short, and
        // there is nothing more to read.
        self.preview.window_eof
    }

    /// Keep the column offset inside the widest line of the visible page.
    fn clamp_preview_column(&mut self) {
        let Some(longest) = preview_visible_width(self) else {
            return
        };

        let max = preview_max_column(longest);

        if self.preview.column > max {
            self.preview.set_column(max);
        }
    }

    /// Scroll the text preview by `delta` lines.
    fn scroll_preview(&mut self, delta: isize) {
        if self.preview.path.is_none() ||
            !matches!(self.file_content, FileContent::Text(_))
        {
            return
        }

        let target = self.preview.offset.saturating_add_signed(delta);

        if let Err(err) = self.show_preview_line(target) {
            self.app_error.add_error(err);
        }
    }

    /// Put `target` at the top of the preview, loading a window around it.
    fn show_preview_line(&mut self, mut target: usize) -> anyhow::Result<()> {
        let page = preview_page();

        if self.preview_window_len() == 0 {
            self.preview.set_offset(0);
            return Ok(())
        }

        for _ in 0..3 {
            if self.preview.window_eof {
                let total = self.preview.window_start +
                    self.preview_window_len();

                target = if total < page { 0 } else { target.min(total - page) };
            }

            if self.preview_window_covers(target, page) {
                self.preview.set_offset(target);
                return Ok(())
            }

            let above = page * PREVIEW_WINDOW_ABOVE_PAGES;
            let ds = target.saturating_sub(above);
            let (line, byte, skip) = self.window_start_for(ds)?;
            self.load_preview_window_skipping(line, byte, skip, true)?;

            if self.preview_window_len() == 0 {
                self.preview.set_offset(0);
                return Ok(())
            }
        }

        // A window which cannot hold a full page (the end of a short file)
        // shows from its first line.
        let start = self.preview.window_start;
        let page = preview_page();

        if !self.preview_window_covers(self.preview.offset, page) {
            self.preview.set_offset(start);
        }

        Ok(())
    }

    /// The start (line, byte and lines to skip) of the window which contains
    /// `line`.
    ///
    /// Lines inside the loaded window are free; earlier ones need a backward
    /// read; a line past the window is reached by skipping the lines between
    /// it and the window's end.
    fn window_start_for(&mut self, line: usize) -> anyhow::Result<(usize, u64, usize)> {
        let start = self.preview.window_start;
        let end = start + self.preview_window_len();

        if line >= start && line < end {
            return Ok((line, self.preview.window_starts[line - start], 0))
        }

        if line == end {
            return Ok((line, self.preview.window_end_byte, 0))
        }

        if line > end {
            return Ok((line, self.preview.window_end_byte, line - end))
        }

        // The line is above the window: read backwards to find its byte.
        let Some(path) = self.preview.path.clone() else {
            return Ok((line, 0, 0))
        };

        let file = fs::File::open(&path)?;
        let prev = read_prev_window(
            &file,
            self.preview.window_start_byte,
            start,
            start - line + 1
        )?;

        if prev.start_line <= line && line - prev.start_line < prev.starts.len()
        {
            Ok((line, prev.starts[line - prev.start_line], 0))
        } else {
            let start_line = prev.start_line;
            let byte = prev.starts
                .first()
                .copied()
                .unwrap_or(self.preview.window_start_byte);

            Ok((start_line, byte, 0))
        }
    }

    /// Jump to `target`, which must be visible after the jump.
    ///
    /// The match is centred when the file allows it: the window starts half a
    /// page before it, which needs a backward read from the match's byte.
    fn preview_jump_to(&mut self, target: PreviewMatch) -> anyhow::Result<()> {
        let page = preview_page();
        let offset = target.line.saturating_sub(page / 2);

        // The target page is already loaded: moving the offset is enough, and
        // the coloured text (and the window) stay as they are, so the jump
        // does not flash.
        if self.preview_window_covers(offset, page) {
            self.preview.set_offset(offset);
            self.preview.set_column(0);

            return Ok(())
        }

        let (start_line, start_byte) = if offset == 0 {
            (0, 0)
        } else if offset == target.line {
            (target.line, target.byte)
        } else {
            let Some(path) = self.preview.path.clone() else {
                return Ok(())
            };

            let file = fs::File::open(&path)?;
            let prev = read_prev_window(
                &file,
                target.byte,
                target.line,
                target.line - offset
            )?;

            if prev.start_line <= offset &&
                offset - prev.start_line < prev.starts.len()
            {
                (offset, prev.starts[offset - prev.start_line])
            } else {
                let start_line = prev.start_line;
                let byte = prev.starts.first().copied().unwrap_or(0);

                (start_line, byte)
            }
        };

        self.load_preview_window_skipping(start_line, start_byte, 0, true)?;

        let mut offset = offset;

        if self.preview.window_eof {
            let total = self.preview.window_start +
                self.preview_window_len();

            offset = if total < page { 0 } else { offset.min(total - page) };
        }

        if offset < self.preview.window_start {
            offset = self.preview.window_start;
        }

        self.preview.set_offset(offset);
        self.preview.set_column(0);

        Ok(())
    }

    /// Load a window which starts `skip` lines after `start_byte`.
    ///
    /// `sync` asks for the window to be highlighted before it is installed: a
    /// scroll or a jump then never paints plain text, only the coloured window.
    /// The first window of a file stays asynchronous, so browsing a directory
    /// does not block on syntect.
    fn load_preview_window_skipping(
        &mut self,
        start_line: usize,
        start_byte: u64,
        skip: usize,
        sync: bool,
    ) -> anyhow::Result<()> {
        let Some(path) = self.preview.path.clone() else {
            return Ok(())
        };

        let file = fs::File::open(&path)?;
        let window = read_window_skipping(
            &file,
            start_byte,
            start_line,
            skip,
            preview_window_capacity()
        )?;

        let content = window.lines.join("\n");
        // The window may start a few lines after `start_byte` (a page far
        // below the loaded one); its context must be read from its own start.
        let window_byte = window.starts
            .first()
            .copied()
            .unwrap_or(start_byte);

        let sync = sync && content.len() < MAX_SYNC_HIGHLIGHT_BYTES;
        let text = if sync {
            match highlight_window(&path, window.start_line, window_byte, &content)
            {
                Some(text) => text,
                None => plain_text(&content)?,
            }
        } else {
            plain_text(&content)?
        };

        let generation = self.preview.install_window(&window, start_byte);
        self.file_content = FileContent::Text(text);

        if !sync {
            self.highlight.request(
                path,
                window.start_line,
                window_byte,
                content,
                generation
            );
        }

        Ok(())
    }

    /// The number of lines in the loaded window.
    fn preview_window_len(&self) -> usize {
        match &self.file_content {
            FileContent::Text(text) => text.lines.len(),
            _ => 0,
        }
    }
}

/// One window height, but never zero (it is the scroll distance of a page).
fn preview_page() -> usize {
    (get_window_height() as usize).max(1)
}

/// How many lines a loaded window holds.
fn preview_window_capacity() -> usize {
    preview_page() * PREVIEW_WINDOW_PAGES
}

/// The width of the widest line of the visible page.
///
/// Only the page is measured: the widest line of the whole file is not known
/// any more, because the rest of it is not loaded.
fn preview_visible_width(app: &App) -> Option<usize> {
    let FileContent::Text(ref text) = app.file_content else {
        return None
    };

    let start = app.preview.offset.saturating_sub(app.preview.window_start);
    let page = preview_page();

    Some(
        text.lines
            .iter()
            .skip(start)
            .take(page)
            .map(|line| line.width())
            .max()
            .unwrap_or(0)
    )
}

/// The largest column the text preview can be scrolled to.
///
/// The end of the widest visible line stays at the right edge of the pane. A
/// line which fits the pane cannot be scrolled at all; the shift is also kept
/// in `u16`, which is what the renderer's scroll offset uses.
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

/// Scan `path` for `query`, a batch of lines at a time.
///
/// The worker reads the file itself, so the UI thread never holds more than a
/// window. A new search bumps the ticket, which stops this scan at the next
/// batch instead of finishing a file the user already left.
fn search_preview_file(
    request: &PreviewSearchRequest,
    ticket: &AtomicU64,
) -> PreviewMatches {
    let mut matches: Vec<PreviewMatch> = Vec::new();
    let mut line = 0;
    let mut byte = 0;

    if let Ok(file) = fs::File::open(&request.path) {
        loop {
            if ticket.load(Ordering::Relaxed) != request.ticket {
                break
            }

            let window = match read_window(
                &file,
                byte,
                line,
                SEARCH_BATCH_LINES
            ) {
                Ok(window) => window,
                Err(_) => break,
            };

            if window.lines.is_empty() {
                break
            }

            let mut full = false;

            for (idx, text) in window.lines.iter().enumerate() {
                if search_indexes(
                    &request.query,
                    std::iter::once(text.as_str()),
                    false
                ).is_empty() {
                    continue
                }

                if matches.len() >= MAX_SEARCH_MATCHES {
                    full = true;
                    break
                }

                matches.push(PreviewMatch {
                    line: window.start_line + idx,
                    byte: window.starts[idx],
                });
            }

            let next_line = window.start_line + window.lines.len();

            if full || window.eof || next_line == line {
                break
            }

            line = next_line;
            byte = window.end_byte;
        }
    }

    PreviewMatches {
        generation: request.generation,
        matches,
    }
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

    fn matched_lines(app: &App) -> Vec<usize> {
        app.preview.matches.iter().map(|found| found.line).collect()
    }

    /// The line `row` of the visible page.
    fn visible_line(app: &App, row: usize) -> String {
        let FileContent::Text(ref text) = app.file_content else {
            return String::new()
        };

        let start = app.preview.offset - app.preview.window_start;

        text.lines[start + row]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// The preview holds only a window of the file, not the whole file.
    #[test]
    fn test_preview_loads_a_window() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("window");
        write_lines(&dir, &numbered_lines(500));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        // Four pages are read: one above the view and two below it.
        assert_eq!(app.preview_window_len(), 40);
        assert!(!app.preview.window_eof);
        assert!(app.preview_window_covers(0, 10));

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A reload is highlighted before it is installed: even without the worker
    /// running, scrolling past the loaded window leaves a coloured preview, so
    /// no plain frame is painted.
    #[test]
    fn test_preview_reload_is_highlighted_synchronously() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("sync-highlight");
        let lines: Vec<String> = (0..200)
            .map(|idx| format!("let value_{idx} = {idx};"))
            .collect();
        fs::write(dir.join("a.rs"), lines.join("\n")).unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        // The first window is left to the worker, which is not started here:
        // it stays plain.
        assert!(!preview_has_colours(&app));

        // Scrolling out of the first window reloads it synchronously.
        for _ in 0..5 {
            app.preview_scroll_page(true);
        }

        assert!(preview_has_colours(&app));
        assert!(!app.highlight.pending());

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Whether the preview already has syntax colours.
    fn preview_has_colours(app: &App) -> bool {
        let FileContent::Text(ref text) = app.file_content else {
            return false
        };

        text.lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.style.fg.is_some())
    }

    /// A page of scrolling fits in the loaded window: it does not reload (and
    /// therefore does not flash) every time.
    #[test]
    fn test_preview_page_down_stays_in_the_window() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("page-window");
        write_lines(&dir, &numbered_lines(500));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        let generation = app.preview.window_generation();

        for _ in 0..3 {
            app.preview_scroll_page(true);
        }

        assert_eq!(app.preview.offset(), 30);
        assert_eq!(app.preview.window_generation(), generation);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A page can be scrolled far past the loaded window; the window is
    /// reloaded around the new position.
    #[test]
    fn test_preview_scrolls_past_the_window() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("far");
        write_lines(&dir, &numbered_lines(500));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        for _ in 0..30 {
            app.preview_scroll_page(true);
        }

        assert_eq!(app.preview.offset(), 300);
        assert_eq!(visible_line(&app, 0), "line 300");

        app.preview_scroll_page(true);
        assert_eq!(app.preview.offset(), 310);
        assert_eq!(visible_line(&app, 0), "line 310");

        // And back up.
        for _ in 0..31 {
            app.preview_scroll_page(false);
        }
        assert_eq!(app.preview.offset(), 0);
        assert_eq!(visible_line(&app, 0), "line 0");

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A taller terminal needs a taller window; the window is reloaded without
    /// moving the view.
    #[test]
    fn test_preview_window_follows_the_terminal_size() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("resize");
        write_lines(&dir, &numbered_lines(200));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        app.preview_scroll_page(true);
        app.preview_scroll_page(true);
        assert_eq!(app.preview.offset(), 20);

        let before = app.preview_window_len();

        // The terminal grew while the preview was open.
        update_window_height(40);
        app.ensure_preview_window(40);

        assert!(app.preview_window_len() > before);
        assert_eq!(app.preview.offset(), 20);
        assert_eq!(visible_line(&app, 0), "line 20");

        // A shorter terminal needs no reload and keeps the view.
        update_window_height(10);
        app.ensure_preview_window(10);
        assert_eq!(app.preview.offset(), 20);
        assert_eq!(visible_line(&app, 0), "line 20");

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A terminal which grows while the last page is on screen pulls the view
    /// up: the last lines are shown from the top of the taller pane, not with
    /// blank rows below the end of the file.
    #[test]
    fn test_preview_growth_at_the_end_pulls_the_view_up() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("resize-end");
        write_lines(&dir, &numbered_lines(50));

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        for _ in 0..10 {
            app.preview_scroll_page(true);
        }
        assert_eq!(app.preview.offset(), 40);

        update_window_height(60);
        app.ensure_preview_window(60);

        assert_eq!(app.preview.offset(), 0);
        assert_eq!(visible_line(&app, 0), "line 0");
        assert_eq!(app.preview_window_len(), 50);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The preview scrolls a line and a page, stays inside its clamps and has
    /// nothing to scroll when it is not text.
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

    /// The column offset moves over the widest visible line five columns at a
    /// time and stops at the right edge of the pane; a line which fits cannot
    /// be scrolled.
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
        assert_eq!(app.preview.column(), 5);

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
        assert_eq!(app.preview.column(), 5);

        // A search jump returns to the first column, so the match is not left
        // hidden behind a horizontal scroll.
        let rx = app.init_preview_search_channel();
        search(&mut app, &rx, "ABCD");
        assert_eq!(matched_lines(&app), vec![0]);
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
        assert_eq!(matched_lines(&app), vec![3, 30]);
        assert_eq!(app.preview.current, Some(0));
        assert_eq!(app.preview.current_line(), Some(3));

        // While the preview is text, `n`/`N` belong to its search.
        assert!(app.preview_search_active());

        // The match is centred: line 3 is shown at the top of a ten-line pane.
        assert_eq!(app.preview.offset(), 0);

        app.preview_move_candidate(true);
        assert_eq!(app.preview.current, Some(1));
        assert_eq!(app.preview.current_line(), Some(30));
        assert_eq!(app.preview.offset(), 25);

        // `n` at the last match wraps around to the first one.
        app.preview_move_candidate(true);
        assert_eq!(app.preview.current, Some(0));
        assert_eq!(app.preview.current_line(), Some(3));
        assert_eq!(app.preview.offset(), 0);

        // `N` at the first match wraps around to the last one.
        app.preview_move_candidate(false);
        assert_eq!(app.preview.current, Some(1));
        assert_eq!(app.preview.current_line(), Some(30));
        assert_eq!(app.preview.offset(), 25);

        // A search without a match keeps the view where it is.
        search(&mut app, &rx, "nothing-matches");
        assert!(app.preview.matches.is_empty());
        assert_eq!(app.preview.current, None);
        assert_eq!(app.preview.current_line(), None);
        assert_eq!(app.preview.offset(), 25);

        // An empty query clears the search, so `n`/`N` leave the preview.
        app.preview_search(String::new()).unwrap();
        assert!(!app.preview.is_searching());
        assert!(!app.preview_search_active());
        assert!(app.preview.matches.is_empty());

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

    /// `n`/`N` between matches which are already in the loaded window only
    /// move the offset: the window (and its coloured text) is kept, so the jump
    /// does not flash.
    #[test]
    fn test_preview_move_candidate_inside_the_window_keeps_it() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("move-window");
        let mut lines = numbered_lines(1000);
        lines[400] = String::from("needle first");
        lines[420] = String::from("needle second");
        write_lines(&dir, &lines);

        let mut app = App::default();
        let rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        search(&mut app, &rx, "needle");
        assert_eq!(matched_lines(&app), vec![400, 420]);
        assert_eq!(app.preview.offset(), 395);

        let generation = app.preview.window_generation();

        app.preview_move_candidate(true);
        assert_eq!(app.preview.offset(), 415);
        assert_eq!(app.preview.window_generation(), generation);

        // And back, still inside the same window.
        app.preview_move_candidate(false);
        assert_eq!(app.preview.offset(), 395);
        assert_eq!(app.preview.window_generation(), generation);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A match outside the loaded window is reached by the byte offset the
    /// scan recorded, without reading the lines before it.
    #[test]
    fn test_preview_search_jumps_out_of_the_window() {
        let _guard = window_size(10, 80);

        let dir = temp_dir("search-jump");
        let mut lines = numbered_lines(1000);
        lines[900] = String::from("needle here");
        write_lines(&dir, &lines);

        let mut app = App::default();
        let rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        search(&mut app, &rx, "needle");
        assert_eq!(matched_lines(&app), vec![900]);
        assert_eq!(app.preview.current, Some(0));
        assert_eq!(app.preview.offset(), 895);
        assert_eq!(visible_line(&app, 5), "needle here");

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

        assert!(app.preview.matches.is_empty());
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

    /// A jump close to the end of the file keeps its view: the last page is
    /// short, and a resize (which calls `ensure_preview_window`) must not pull
    /// the view back to the start of the window.
    #[test]
    fn test_preview_jump_near_the_end_survives_a_resize() {
        let _guard = window_size(34, 120);

        let dir = temp_dir("jump-end");
        let mut lines: Vec<String> = (0..200)
            .map(|idx| format!("{idx:03} {}", "x".repeat(60)))
            .collect();
        lines[120] = format!("120 needle first {}", "x".repeat(60));
        lines[170] = format!("170 needle second {}", "x".repeat(60));
        write_lines(&dir, &lines);

        let mut app = App::default();
        let rx = app.init_preview_search_channel();
        app.goto_dir(&dir, Some(false)).unwrap();

        search(&mut app, &rx, "needle");
        app.preview_move_candidate(true);
        assert_eq!(app.preview.offset(), 170 - 17);
        assert_eq!(visible_line(&app, 0).split(' ').next().unwrap(), "153");

        // The resize needs one more line than the window was read with.
        update_window_height(35);
        app.ensure_preview_window(35);
        assert_eq!(app.preview.offset(), 153);

        update_window_height(0);
        update_window_width(0);
        fs::remove_dir_all(&dir).unwrap();
    }
}
