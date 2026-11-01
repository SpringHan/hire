// Graphics support for shell sessions.
//
// The child of a shell session talks to a real PTY, so a TUI inside it probes
// the terminal for image support and then paints pictures with the kitty
// graphics protocol. `vt100` models a text grid only, so those escape sequences
// would be dropped and the pictures never reach the real terminal.
//
// This module splits the kitty sequences out of the byte stream before the
// parser sees them, answers the child's capability queries, and hands the image
// data to the main loop, which forwards it to the real terminal. The child (and
// hire's own renderer) already paints the unicode placeholder cells that make
// the terminal draw the image, so only the image data has to be forwarded.
//
// Unlike hire's own preview, this path does not decode or re-encode anything:
// the child sized the image for the terminal's real cell size, and the terminal
// draws it where the placeholders are. Nothing else about the shell (text,
// colours, alt screen) is affected.

use std::collections::HashSet;

/// The kitty graphics delete command which frees an image id and its
/// placements. Used to translate the child's "delete everything" without
/// touching the images of hire's own preview.
const DELETE_IMAGE: &str = "\x1b_Ga=d,d=I,q=2,i=";

/// Never grow the forwarding buffer beyond this; a broken `m=1` chunk stream
/// would otherwise be remembered forever.
const MAX_FORWARD_BYTES: usize = 32 * 1024 * 1024;

/// Bytes of a text run kept back so a query split between two reads is found.
const QUERY_TAIL: usize = 5;

/// What the child is told about the terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsSupport {
    /// The real terminal speaks the kitty graphics protocol.
    pub kitty: bool,

    /// Cell size in pixels (width, height); `(0, 0)` when it is unknown.
    pub cell: (u16, u16),
}

impl GraphicsSupport {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn kitty(cell: (u16, u16)) -> Self {
        Self {
            kitty: cell.0 > 0 && cell.1 > 0,
            cell,
        }
    }

    pub fn enabled(&self) -> bool {
        self.kitty
    }
}

/// Something the main loop has to do for the shell the event belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphicsEvent {
    /// Write these bytes to the child's PTY (the answer to a query).
    Reply(Vec<u8>),

    /// Write these bytes to the real terminal (kitty image data or deletes).
    Forward(Vec<u8>),
}

/// A kitty image transmission which is being collected chunk by chunk.
struct Forwarding {
    id: Option<u32>,
    virtual_placement: bool,
    bytes: Vec<u8>,
}

/// Splits kitty graphics sequences out of a shell's output and answers the
/// terminal capability queries the child sends.
pub struct GraphicsFilter {
    support: GraphicsSupport,
    /// A partial APC sequence, from `ESC _` onwards.
    pending: Vec<u8>,
    /// The tail of the last text run, so split queries are still found.
    tail: Vec<u8>,
    /// The transmission currently being collected for the real terminal.
    forward: Option<Forwarding>,
    /// Image ids whose data was forwarded to the real terminal.
    forwarded: HashSet<u32>,
}

impl GraphicsFilter {
    pub fn new(support: GraphicsSupport) -> Self {
        Self {
            support,
            pending: Vec::new(),
            tail: Vec::new(),
            forward: None,
            forwarded: HashSet::new(),
        }
    }

    /// Split `bytes` into text for the vt100 parser and graphics events.
    ///
    /// `rows`/`cols` are the child's terminal size, used by the size replies.
    pub fn feed(
        &mut self,
        bytes: &[u8],
        rows: u16,
        cols: u16,
        text: &mut dyn FnMut(&[u8]),
        emit: &mut dyn FnMut(GraphicsEvent),
    ) {
        self.pending.extend_from_slice(bytes);

        loop {
            let Some(pos) = find(&self.pending, b"\x1b_") else {
                // Plain text. A trailing `ESC` may be the start of an APC that
                // is still on its way, so it is kept for the next read.
                let hold = usize::from(self.pending.last() == Some(&0x1b));
                let split = self.pending.len() - hold;
                let chunk = self.pending[..split].to_vec();
                self.pending.drain(..split);

                if !chunk.is_empty() {
                    self.text(&chunk, rows, cols, text, emit);
                }

                break
            };

            let chunk = self.pending[..pos].to_vec();
            self.pending.drain(..pos);

            if !chunk.is_empty() {
                self.text(&chunk, rows, cols, text, emit);
            }

            // The APC body ends at `ESC \`; wait for the rest of it. vt100
            // ignores the whole sequence, so nothing is fed to it here.
            let Some(end) = find(&self.pending[2..], b"\x1b\\") else {
                break
            };
            let body = self.pending[2..2 + end].to_vec();
            self.pending.drain(..2 + end + 2);

            self.apc(&body, emit);
        }
    }

    /// Feed one text run to the parser and answer the queries inside it.
    fn text(
        &mut self,
        chunk: &[u8],
        rows: u16,
        cols: u16,
        text: &mut dyn FnMut(&[u8]),
        emit: &mut dyn FnMut(GraphicsEvent),
    ) {
        text(chunk);

        if !self.support.enabled() {
            self.tail.clear();
            return
        }

        let overlap = self.tail.len();
        let mut scan = std::mem::take(&mut self.tail);
        scan.extend_from_slice(chunk);

        let mut idx = 0;
        while idx < scan.len() {
            let Some((len, reply)) = self.query(&scan[idx..], rows, cols) else {
                idx += 1;
                continue
            };

            // A match which already ended inside the previous tail was seen
            // last time; reporting it again would answer the query twice.
            if idx + len > overlap {
                emit(GraphicsEvent::Reply(reply));
            }

            idx += len;
        }

        let keep = scan.len().min(QUERY_TAIL);
        self.tail = scan[scan.len() - keep..].to_vec();
    }

    /// Match one terminal query and build its answer.
    fn query(&self, buf: &[u8], rows: u16, cols: u16) -> Option<(usize, Vec<u8>)> {
        let (pattern, reply) = if buf.starts_with(b"\x1b[16t") {
            // Cell size in pixels: `CSI 6 ; height ; width t`.
            (
                5,
                format!("\x1b[6;{};{}t", self.support.cell.1, self.support.cell.0),
            )
        } else if buf.starts_with(b"\x1b[14t") {
            // Text area size in pixels.
            (
                5,
                format!(
                    "\x1b[4;{};{}t",
                    rows.saturating_mul(self.support.cell.1),
                    cols.saturating_mul(self.support.cell.0)
                ),
            )
        } else if buf.starts_with(b"\x1b[0c") || buf.starts_with(b"\x1b[c") {
            // Primary device attributes. No sixel, so the child picks kitty.
            let len = if buf.starts_with(b"\x1b[0c") { 4 } else { 3 };
            (len, String::from("\x1b[?62;22c"))
        } else if buf.starts_with(b"\x1b[5n") {
            // Device status report: "terminal ready".
            (4, String::from("\x1b[0n"))
        } else {
            return None
        };

        Some((pattern, reply.into_bytes()))
    }

    /// Handle one APC sequence (the bytes between `ESC _` and `ESC \`).
    fn apc(&mut self, body: &[u8], emit: &mut dyn FnMut(GraphicsEvent)) {
        let Some(rest) = body.strip_prefix(b"G") else {
            return
        };

        let (params, payload) = match rest.iter().position(|byte| *byte == b';') {
            Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
            None => (rest, None),
        };

        let action = param(params, b"a");
        // A follow-up chunk of a chunked transmission carries only `m`.
        let continuation = action.is_none() && param(params, b"m").is_some();

        if action == Some(b"q".as_slice()) {
            self.answer(params, emit);
        } else if action == Some(b"T".as_slice()) ||
            action == Some(b"t".as_slice()) ||
            continuation
        {
            self.transmit(params, payload, emit);
        } else if action == Some(b"d".as_slice()) {
            self.delete(params, payload, emit);
        }
        // Anything else (a direct placement, an animation frame, ...) is
        // dropped: hire cannot map the child's cursor to the real terminal.
    }

    /// Answer a kitty graphics query for the child.
    fn answer(&self, params: &[u8], emit: &mut dyn FnMut(GraphicsEvent)) {
        if !self.support.enabled() {
            return
        }

        let id = param(params, b"i")
            .and_then(parse_u32)
            .unwrap_or_default();
        // Only direct transmission is supported: hire reads the image data
        // from the PTY, so shared memory, temporary files and file paths are
        // refused and the child sends the bytes inline.
        let media = param(params, b"t");
        let status = if media.is_none() || media == Some(b"d".as_slice()) {
            "OK"
        } else {
            "ENOTSUP"
        };

        emit(GraphicsEvent::Reply(
            format!("\x1b_Gi={id};{status}\x1b\\").into_bytes(),
        ));
    }

    /// Collect a (possibly chunked) image transmission and forward it whole.
    fn transmit(
        &mut self,
        params: &[u8],
        payload: Option<&[u8]>,
        emit: &mut dyn FnMut(GraphicsEvent),
    ) {
        let id = param(params, b"i").and_then(parse_u32);
        let more = param(params, b"m") == Some(b"1".as_slice());
        let virtual_placement = param(params, b"U") == Some(b"1".as_slice());
        let starts = param(params, b"a").is_some();

        if starts {
            // A new transmission replaces a half-received previous one.
            self.forward = None;

            // Only placeholder images can be forwarded: the child positions
            // them itself by painting placeholder cells.
            if !self.support.enabled() || !virtual_placement {
                return
            }

            self.forward = Some(Forwarding {
                id,
                virtual_placement,
                bytes: Vec::new(),
            });
        }

        let Some(forward) = self.forward.as_mut() else {
            return
        };

        forward.bytes.extend_from_slice(&command(params, payload));

        if forward.bytes.len() > MAX_FORWARD_BYTES {
            self.forward = None;
            return
        }

        if more {
            return
        }

        let forward = self.forward.take().expect("the forwarding just existed");

        if !forward.virtual_placement {
            return
        }

        if let Some(id) = forward.id {
            self.forwarded.insert(id);
        }

        emit(GraphicsEvent::Forward(forward.bytes));
    }

    /// Translate a delete command so it can only affect the images this shell
    /// forwarded, never hire's own preview.
    fn delete(
        &mut self,
        params: &[u8],
        _payload: Option<&[u8]>,
        emit: &mut dyn FnMut(GraphicsEvent),
    ) {
        if !self.support.enabled() {
            return
        }

        let spec = param(params, b"d");

        if spec == Some(b"A".as_slice()) || spec == Some(b"a".as_slice()) {
            // "delete everything": the child assumes it owns the terminal, but
            // it does not, so delete each forwarded image by id.
            let ids = self.forwarded.drain().collect::<Vec<_>>();

            for id in ids {
                emit(GraphicsEvent::Forward(
                    format!("{DELETE_IMAGE}{id}\x1b\\").into_bytes(),
                ));
            }
        } else if spec == Some(b"I".as_slice()) || spec == Some(b"i".as_slice()) {
            // Delete one image/placement, but only when it is ours.
            let Some(id) = param(params, b"i").and_then(parse_u32) else {
                return
            };

            if self.forwarded.remove(&id) {
                emit(GraphicsEvent::Forward(command(params, _payload)));
            }
        }
    }
}

/// The value of one `key=value` parameter of a kitty command.
fn param<'a>(params: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    for part in params.split(|byte| *byte == b',') {
        let Some(eq) = part.iter().position(|byte| *byte == b'=') else {
            continue
        };

        if &part[..eq] == key {
            return Some(&part[eq + 1..])
        }
    }

    None
}

fn parse_u32(value: &[u8]) -> Option<u32> {
    std::str::from_utf8(value).ok()?.parse().ok()
}

/// Rebuild a kitty command with `q=2`, so the real terminal never answers it.
///
/// The terminal's answer would arrive on hire's own stdin, where crossterm would
/// read it as key presses for the attached shell.
fn command(params: &[u8], payload: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(params.len() + 16);
    out.extend_from_slice(b"\x1b_G");
    out.extend_from_slice(&quiet_params(params));

    if let Some(payload) = payload {
        out.push(b';');
        out.extend_from_slice(payload);
    }

    out.extend_from_slice(b"\x1b\\");
    out
}

fn quiet_params(params: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(params.len() + 4);
    let mut quiet = false;

    for part in params.split(|byte| *byte == b',') {
        if part.is_empty() {
            continue
        }

        if !out.is_empty() {
            out.push(b',');
        }

        let key = part.iter().position(|byte| *byte == b'=').map(|eq| &part[..eq]);

        if key == Some(b"q".as_slice()) {
            out.extend_from_slice(b"q=2");
            quiet = true;
        } else {
            out.extend_from_slice(part);
        }
    }

    if !quiet {
        if !out.is_empty() {
            out.push(b',');
        }
        out.extend_from_slice(b"q=2");
    }

    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None
    }

    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod test {
    use super::*;

    fn support() -> GraphicsSupport {
        GraphicsSupport::kitty((10, 20))
    }

    /// Feed `bytes` and return `(text, events)`.
    fn feed(
        filter: &mut GraphicsFilter,
        bytes: &[u8],
        rows: u16,
        cols: u16,
    ) -> (Vec<u8>, Vec<GraphicsEvent>) {
        let mut text = Vec::new();
        let mut events = Vec::new();

        filter.feed(
            bytes,
            rows,
            cols,
            &mut |chunk| text.extend_from_slice(chunk),
            &mut |event| events.push(event),
        );

        (text, events)
    }

    fn forwarded(events: &[GraphicsEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|event| match event {
                GraphicsEvent::Forward(bytes) => Some(bytes.clone()),
                GraphicsEvent::Reply(_) => None,
            })
            .flatten()
            .collect()
    }

    fn replies(events: &[GraphicsEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|event| match event {
                GraphicsEvent::Reply(bytes) => Some(bytes.clone()),
                GraphicsEvent::Forward(_) => None,
            })
            .flatten()
            .collect()
    }

    /// Kitty image data is taken out of the text stream; everything else is
    /// still parsed as text.
    #[test]
    fn test_image_data_is_not_text() {
        let mut filter = GraphicsFilter::new(support());
        let (text, events) = feed(
            &mut filter,
            b"AB\x1b_Gq=2,a=T,U=1,i=7,f=24,s=2,v=2;AAAA\x1b\\CD",
            10,
            40,
        );

        assert_eq!(text, b"ABCD");
        assert_eq!(
            forwarded(&events),
            b"\x1b_Gq=2,a=T,U=1,i=7,f=24,s=2,v=2;AAAA\x1b\\"
        );
    }

    /// The image data is split over several APCs (`m=1` ... `m=0`) and is
    /// forwarded as one sequence when the last chunk arrives.
    #[test]
    fn test_chunked_transmission() {
        let mut filter = GraphicsFilter::new(support());

        let (_, events) = feed(&mut filter, b"\x1b_Ga=T,U=1,i=9,f=24,m=1;AAAA\x1b\\", 10, 40);
        assert!(events.is_empty());

        let (_, events) = feed(&mut filter, b"\x1b_Gm=1;BBBB\x1b\\", 10, 40);
        assert!(events.is_empty());

        let (text, events) = feed(&mut filter, b"\x1b_Gm=0;CCCC\x1b\\", 10, 40);
        assert!(text.is_empty());
        assert_eq!(
            forwarded(&events),
            b"\x1b_Ga=T,U=1,i=9,f=24,m=1,q=2;AAAA\x1b\\\x1b_Gm=1,q=2;BBBB\x1b\\\x1b_Gm=0,q=2;CCCC\x1b\\"
        );
    }

    /// A split APC is reassembled; a split query is answered exactly once.
    #[test]
    fn test_split_sequences_across_reads() {
        let mut filter = GraphicsFilter::new(support());

        let (text, events) = feed(&mut filter, b"x\x1b_Ga=T,U=1,i=3,f=24;AA", 10, 40);
        assert_eq!(text, b"x");
        assert_eq!(replies(&events), b"");
        assert_eq!(forwarded(&events), b"");

        let (_, events) = feed(&mut filter, b"BB\x1b\\y", 10, 40);
        assert_eq!(
            forwarded(&events),
            b"\x1b_Ga=T,U=1,i=3,f=24,q=2;AABB\x1b\\"
        );

        // `CSI 16 t` split between the two reads is answered once.
        let (_, events) = feed(&mut filter, b"\x1b[1", 10, 40);
        assert_eq!(replies(&events), b"");
        let (_, events) = feed(&mut filter, b"6t", 10, 40);
        assert_eq!(replies(&events), b"\x1b[6;20;10t");
    }

    #[test]
    fn test_capability_queries_are_answered() {
        let mut filter = GraphicsFilter::new(support());
        let (_, events) = feed(
            &mut filter,
            b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c\x1b[16t\x1b[14t",
            24,
            80,
        );

        let replies = String::from_utf8(replies(&events)).unwrap();
        println!("REPLIES: {replies:?}");
        assert!(replies.contains("\x1b_Gi=31;OK\x1b\\"));
        assert!(replies.contains("\x1b[?62;22c"));
        assert!(replies.contains("\x1b[6;20;10t"));
        assert!(replies.contains("\x1b[4;480;800t"));
    }

    /// Only direct transmission is accepted, so the child sends the data
    /// inline instead of shared memory or file paths.
    #[test]
    fn test_only_direct_transmission_is_accepted() {
        let mut filter = GraphicsFilter::new(support());

        for media in [b"s", b"f", b"t"] {
            let query = format!(
                "\x1b_Gi=12,s=1,v=1,a=q,t={},f=24,S=3;AAAA\x1b\\",
                String::from_utf8_lossy(media)
            );
            let (_, events) = feed(&mut filter, query.as_bytes(), 10, 40);

            assert_eq!(replies(&events), b"\x1b_Gi=12;ENOTSUP\x1b\\");
        }

        let (_, events) = feed(
            &mut filter,
            b"\x1b_Gi=12,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\",
            10,
            40,
        );
        assert_eq!(replies(&events), b"\x1b_Gi=12;OK\x1b\\");
    }

    /// A direct placement is dropped: hire cannot map the terminal cursor to
    /// the child's cursor.
    #[test]
    fn test_direct_placement_is_dropped() {
        let mut filter = GraphicsFilter::new(support());
        let (_, events) = feed(&mut filter, b"\x1b_Ga=T,f=24,i=4;AAAA\x1b\\", 10, 40);

        assert_eq!(forwarded(&events), b"");
    }

    /// "Delete all" only removes the images this shell forwarded.
    #[test]
    fn test_delete_all_only_touches_forwarded_images() {
        let mut filter = GraphicsFilter::new(support());
        feed(&mut filter, b"\x1b_Ga=T,U=1,i=5,f=24;AAAA\x1b\\", 10, 40);
        feed(&mut filter, b"\x1b_Ga=T,U=1,i=6,f=24;AAAA\x1b\\", 10, 40);

        let (_, events) = feed(&mut filter, b"\x1b_Ga=d,d=A\x1b\\", 10, 40);
        let mut forwards = events
            .iter()
            .filter_map(|event| match event {
                GraphicsEvent::Forward(bytes) => Some(String::from_utf8(bytes.clone()).unwrap()),
                GraphicsEvent::Reply(_) => None,
            })
            .collect::<Vec<_>>();
        forwards.sort();

        assert_eq!(forwards.len(), 2);
        assert!(forwards.iter().any(|f| f.contains("i=5")));
        assert!(forwards.iter().any(|f| f.contains("i=6")));

        // A second delete has nothing left to delete.
        let (_, events) = feed(&mut filter, b"\x1b_Ga=d,d=A\x1b\\", 10, 40);
        assert_eq!(forwarded(&events), b"");
    }

    /// A delete for an id this shell never sent is ignored, so a TUI inside a
    /// shell cannot delete hire's own preview image.
    #[test]
    fn test_delete_of_unknown_id_is_dropped() {
        let mut filter = GraphicsFilter::new(support());
        let (_, events) = feed(&mut filter, b"\x1b_Ga=d,d=I,i=99\x1b\\", 10, 40);

        assert_eq!(forwarded(&events), b"");
    }

    /// Without a kitty capable terminal nothing is answered or forwarded, and
    /// the text still reaches the parser.
    #[test]
    fn test_disabled_support_keeps_the_text() {
        let mut filter = GraphicsFilter::new(GraphicsSupport::disabled());
        let (text, events) = feed(
            &mut filter,
            b"AB\x1b_Gi=31,a=q;AAAA\x1b\\\x1b[cCD",
            10,
            40,
        );

        // The APC is dropped, but the CSI query is ordinary text for vt100.
        assert_eq!(text, b"AB\x1b[cCD");
        assert!(replies(&events).is_empty());
        assert!(forwarded(&events).is_empty());
    }
}
