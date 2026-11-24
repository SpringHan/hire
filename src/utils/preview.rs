//! Reading the text preview from a file, a window at a time.

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
};

/// The number of bytes read to decide whether a file is text.
///
/// Binary data gives itself away in the first kilobytes; the rest is read
/// window by window when the preview needs it.
pub const PREVIEW_PROBE_BYTES: usize = 64 * 1024;

/// The longest line the preview keeps, in bytes.
///
/// A minified document can put the whole file on one line, which would then be
/// loaded although the terminal shows only a screenful of columns.
pub const MAX_PREVIEW_LINE_BYTES: usize = 16 * 1024;

/// How many bytes are read at a time when the preview needs earlier lines.
const BACK_BLOCK_BYTES: usize = 64 * 1024;

/// How far back [`read_prev_window`] reads to find the requested lines.
const MAX_BACK_BYTES: usize = 4 * 1024 * 1024;

/// A run of complete lines read from a file.
#[derive(Debug, Default)]
pub struct PreviewWindow {
    /// The lines, without their line break. A line is at most
    /// [`MAX_PREVIEW_LINE_BYTES`] bytes of its file line.
    pub lines: Vec<String>,

    /// The byte offset every line starts at.
    pub starts: Vec<u64>,

    /// The absolute line index of `lines[0]`.
    pub start_line: usize,

    pub end_byte: u64,

    pub eof: bool,
}

/// Read up to [`PREVIEW_PROBE_BYTES`] bytes of `file` as UTF-8.
///
/// CR bytes are dropped, so a CRLF file renders like an LF one. Invalid UTF-8
/// within the bytes read is an error, which the preview takes as "not text". A
/// multi-byte character cut in half by the limit is dropped rather than making
/// the whole file look like binary data.
pub fn read_probe(file: &File) -> io::Result<String> {
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(0))?;

    let mut bytes: Vec<u8> = Vec::with_capacity(PREVIEW_PROBE_BYTES);
    let mut read = 0;
    let mut chunk = [0u8; 8 * 1024];

    while read < PREVIEW_PROBE_BYTES {
        let want = (PREVIEW_PROBE_BYTES - read).min(chunk.len());
        let got = reader.read(&mut chunk[..want])?;
        if got == 0 {
            break
        }

        bytes.extend(chunk[..got].iter().copied().filter(|byte| *byte != 13));
        read += got;
    }

    let at_limit = read == PREVIEW_PROBE_BYTES;

    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(err) if at_limit && err.utf8_error().error_len().is_none() => {
            let valid = err.utf8_error().valid_up_to();
            let bytes = err.into_bytes();

            Ok(String::from_utf8(bytes[..valid].to_vec())
                .expect("the bytes before the cut are valid UTF-8"))
        },
        Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
    }
}

/// Read up to `count` lines of `file`, starting at `start_byte`.
///
/// `start_line` is the absolute line index of `start_byte`. A file which ends
/// earlier yields fewer lines and `eof`.
pub fn read_window(
    file: &File,
    start_byte: u64,
    start_line: usize,
    count: usize,
) -> io::Result<PreviewWindow>
{
    read_window_skipping(file, start_byte, start_line, 0, count)
}

/// Read a window which starts `skip` lines after `start_byte`.
///
/// The skipped lines are still read (the only way to reach the next line) but
/// not returned; a scroll far below the loaded window uses this.
pub fn read_window_skipping(
    file: &File,
    start_byte: u64,
    start_line: usize,
    skip: usize,
    count: usize,
) -> io::Result<PreviewWindow>
{
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_byte))?;

    let mut lines: Vec<String> = Vec::with_capacity(count);
    let mut starts: Vec<u64> = Vec::with_capacity(count);
    let mut pos = start_byte;
    let mut read_lines = 0;
    let mut eof = false;

    for idx in 0..skip.saturating_add(count) {
        match read_line(&mut reader)? {
            Some((line, consumed)) => {
                if idx >= skip {
                    lines.push(line);
                    starts.push(pos);
                }

                pos += consumed;
                read_lines += 1;
            },
            None => {
                eof = true;
                break
            },
        }
    }

    // The skip may have run past the end of the file; then the window is empty
    // and starts at the last line.
    let skipped = skip.min(read_lines);

    Ok(PreviewWindow {
        lines,
        starts,
        start_line: start_line.saturating_add(skipped),
        end_byte: pos,
        eof,
    })
}

/// Read up to `count` lines which end at `end_byte`.
///
/// `end_byte` is the first byte of a line (or the start of the file), so the
/// bytes before it end with a line break; this is what a scroll upwards (and
/// the highlight context) needs. `end_line` is the absolute line index of
/// `end_byte`.
///
/// At most [`MAX_BACK_BYTES`] are read, so a file with longer lines yields
/// fewer lines — harmless for a preview.
pub fn read_prev_window(
    file: &File,
    end_byte: u64,
    end_line: usize,
    count: usize,
) -> io::Result<PreviewWindow> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut start = end_byte;
    let mut newlines = 0;
    let mut total = 0;

    while start > 0 && newlines < count.saturating_add(1) &&
        total < MAX_BACK_BYTES
    {
        let step = (start as usize)
            .min(BACK_BLOCK_BYTES)
            .min(MAX_BACK_BYTES - total);
        let from = start - step as u64;

        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(from))?;

        let mut chunk = vec![0u8; step];
        reader.read_exact(&mut chunk)?;

        newlines += chunk.iter().filter(|byte| **byte == b'\n').count();
        total += step;
        start = from;
        blocks.push(chunk);
    }

    let mut bytes: Vec<u8> = Vec::with_capacity(total);
    for block in blocks.iter().rev() {
        bytes.extend_from_slice(block);
    }

    // Split `[start, end_byte)` into lines: a partial line at the front is not
    // a line the window can start at.
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let mut from = 0;

    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            pieces.push((from, idx));
            from = idx + 1;
        }
    }

    if from < bytes.len() {
        pieces.push((from, bytes.len()));
    }

    if start > 0 && !pieces.is_empty() {
        pieces.remove(0);
    }

    let keep = pieces.len().min(count);
    let pieces = &pieces[pieces.len() - keep..];

    let mut window = PreviewWindow {
        lines: Vec::with_capacity(keep),
        starts: Vec::with_capacity(keep),
        start_line: end_line.saturating_sub(keep),
        end_byte,
        eof: false,
    };

    for (begin, end) in pieces {
        let mut line = &bytes[*begin..*end];

        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }

        line = &line[..line.len().min(MAX_PREVIEW_LINE_BYTES)];

        window.starts.push(start + *begin as u64);
        window.lines.push(String::from_utf8_lossy(line).into_owned());
    }

    Ok(window)
}

/// Read one line of `reader`, without its line break.
///
/// The returned number counts the bytes the line occupied in the file, its line
/// break and any part the limit cut off. Invalid UTF-8 is replaced rather than
/// failing: the probe already decided the file is text, and one bad byte must
/// not end the preview.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<(String, u64)>> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut consumed = 0;
    let mut truncated = false;
    let mut any = false;

    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            break
        }

        any = true;

        let newline = buf.iter().position(|byte| *byte == b'\n');
        let end = newline.map(|idx| idx + 1).unwrap_or(buf.len());

        if !truncated {
            let room = MAX_PREVIEW_LINE_BYTES.saturating_sub(bytes.len());
            let take = end.min(room);

            bytes.extend_from_slice(&buf[..take]);

            if take < end {
                truncated = true;
            }
        }

        consumed += end as u64;
        reader.consume(end);

        if newline.is_some() {
            break
        }
    }

    if !any {
        return Ok(None)
    }

    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }

    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }

    Ok(Some((String::from_utf8_lossy(&bytes).into_owned(), consumed)))
}

#[cfg(test)]
mod test {
    use std::{
        fs::{self, File},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-preview-read-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn numbered(dir: &Path, count: usize) {
        let text: Vec<String> = (0..count).map(|idx| format!("line {idx}")).collect();
        fs::write(dir.join("a.txt"), text.join("\n")).unwrap();
    }

    #[test]
    fn test_read_window_forward() {
        let dir = temp_dir("forward");
        numbered(&dir, 10);

        let file = File::open(dir.join("a.txt")).unwrap();

        let window = read_window(&file, 0, 0, 4).unwrap();
        assert_eq!(window.lines, ["line 0", "line 1", "line 2", "line 3"]);
        assert_eq!(window.start_line, 0);
        assert_eq!(window.starts, [0, 7, 14, 21]);
        assert!(!window.eof);

        // The byte offset of a later line is the seek position of a window.
        let window = read_window(
            &file,
            *window.starts.last().unwrap(),
            3,
            10
        ).unwrap();
        assert_eq!(window.lines[0], "line 3");
        assert_eq!(window.lines.len(), 7);
        assert!(window.eof);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A skipped window keeps the absolute numbering and the byte offsets.
    #[test]
    fn test_read_window_skipping() {
        let dir = temp_dir("skip");
        numbered(&dir, 10);

        let file = File::open(dir.join("a.txt")).unwrap();
        let window = read_window_skipping(&file, 0, 0, 5, 2).unwrap();

        assert_eq!(window.lines, ["line 5", "line 6"]);
        assert_eq!(window.start_line, 5);
        assert_eq!(window.starts, [35, 42]);

        let window = read_window_skipping(&file, 0, 0, 20, 2).unwrap();
        assert!(window.lines.is_empty());
        assert!(window.eof);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_prev_window() {
        let dir = temp_dir("back");
        numbered(&dir, 10);

        let file = File::open(dir.join("a.txt")).unwrap();
        let end = 35; // the start of "line 5"

        let window = read_prev_window(&file, end, 5, 2).unwrap();
        assert_eq!(window.lines, ["line 3", "line 4"]);
        assert_eq!(window.start_line, 3);
        assert_eq!(window.starts, [21, 28]);

        let window = read_prev_window(&file, 0, 0, 2).unwrap();
        assert!(window.lines.is_empty());

        let window = read_prev_window(&file, end, 5, 100).unwrap();
        assert_eq!(window.lines.len(), 5);
        assert_eq!(window.start_line, 0);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_window_crlf_and_empty() {
        let dir = temp_dir("crlf");
        fs::write(dir.join("crlf.txt"), "one\r\ntwo\r\n").unwrap();
        fs::write(dir.join("empty.txt"), "").unwrap();

        let file = File::open(dir.join("crlf.txt")).unwrap();
        let window = read_window(&file, 0, 0, 10).unwrap();
        assert_eq!(window.lines, ["one", "two"]);
        assert!(window.eof);

        let file = File::open(dir.join("empty.txt")).unwrap();
        let window = read_window(&file, 0, 0, 10).unwrap();
        assert!(window.lines.is_empty());
        assert!(window.eof);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A line which is longer than the limit is cut, and the rest of the line
    /// is skipped so the following lines are still reachable.
    #[test]
    fn test_long_line_is_cut() {
        let dir = temp_dir("long-line");

        let mut text = "x".repeat(MAX_PREVIEW_LINE_BYTES * 2);
        text.push_str("\nafter\n");
        fs::write(dir.join("long.txt"), text).unwrap();

        let file = File::open(dir.join("long.txt")).unwrap();
        let window = read_window(&file, 0, 0, 2).unwrap();

        assert_eq!(window.lines[0].len(), MAX_PREVIEW_LINE_BYTES);
        assert_eq!(window.lines[1], "after");
        assert_eq!(
            window.starts[1],
            (MAX_PREVIEW_LINE_BYTES * 2 + 1) as u64
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_probe() {
        let dir = temp_dir("probe");

        fs::write(dir.join("text.txt"), "hello\nworld\n").unwrap();
        fs::write(dir.join("binary.bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();

        let file = File::open(dir.join("text.txt")).unwrap();
        assert_eq!(read_probe(&file).unwrap(), "hello\nworld\n");

        let file = File::open(dir.join("binary.bin")).unwrap();
        assert!(read_probe(&file).is_err());

        // The limit may cut the last character of the probe in half.
        let mut content = "a".repeat(PREVIEW_PROBE_BYTES - 1).into_bytes();
        content.extend_from_slice("中".as_bytes());
        fs::write(dir.join("cut.txt"), content).unwrap();

        let file = File::open(dir.join("cut.txt")).unwrap();
        assert_eq!(read_probe(&file).unwrap().len(), PREVIEW_PROBE_BYTES - 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Lines of different lengths keep their byte offsets, so a backward read
    /// returns exactly the lines above the window.
    #[test]
    fn test_read_prev_window_of_mixed_lines() {
        let dir = temp_dir("e2e");
        let tail = "TAILMARKER";
        let mut content = String::new();

        for idx in 0..200 {
            let marker = ((b'A' + (idx % 26) as u8) as char)
                .to_string()
                .repeat(16);
            let padding = "x".repeat(40);
            let line = match idx {
                120 => format!("{marker} needle first {padding}{tail}"),
                170 => format!("{marker} needle second {padding}{tail}"),
                _ => format!("{marker} filler {idx} {padding}{tail}"),
            };
            content.push_str(&line);
            content.push('\n');
        }

        fs::write(dir.join("a.txt"), &content).unwrap();
        let file = File::open(dir.join("a.txt")).unwrap();

        let window = read_window(&file, 0, 0, 200).unwrap();
        assert_eq!(window.lines.len(), 200, "{:?}", window.lines.len());

        let byte_170 = window.starts[170];
        let byte_153 = window.starts[153];
        assert_eq!(
            content[..byte_170 as usize].matches('\n').count(),
            170
        );

        let prev = read_prev_window(&file, byte_170, 170, 17).unwrap();
        assert_eq!(prev.start_line, 153);
        assert_eq!(prev.starts[0], byte_153);
        assert_eq!(prev.lines[0], window.lines[153]);

        fs::remove_dir_all(&dir).unwrap();
    }
}
