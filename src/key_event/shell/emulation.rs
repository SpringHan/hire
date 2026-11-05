// Terminal emulation fixes for the child's screen.
//
// `vt100` parses the child's output into the grid hire renders, but it does not
// implement every sequence a real terminal understands. The child's PTY is
// emulated by hire, so anything vt100 drops changes what the program inside the
// shell sees: a missing cursor movement moves the program's whole screen.
//
// This module translates the output before vt100 parses it. Translation keeps
// every byte count, so the graphics filter can still split the stream at its
// queries.

/// The sequences translated so far, one state machine per shell session.
///
/// `vt100` has no `CSI f` (HVP, horizontal and vertical position), although a
/// terminal treats it exactly like `CSI H` (CUP). Programs which cannot read
/// the terminal size any other way move the cursor to the bottom right with HVP
/// and then ask for its position (`CSI 6 n`); `gh`'s prompts do exactly that.
/// Without the translation the cursor stays where it was, so the position
/// report and everything the program paints afterwards are wrong.
#[derive(Default, Debug)]
pub struct Hvp {
    state: State,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Ordinary text.
    #[default]
    Ground,

    /// An `ESC` was read: `[` starts a CSI, some other byte ends the escape.
    Escape,

    /// Inside `CSI parameters intermediates`, until a final byte.
    Csi,

    /// Inside an OSC string, which ends with `BEL` or `ESC \`.
    Osc,

    /// An `ESC` was read inside an OSC string.
    OscEscape,

    /// Inside a DCS/SOS/PM/APC string, which ends with `ESC \`.
    String,

    /// An `ESC` was read inside a DCS/SOS/PM/APC string.
    StringEscape,
}

impl Hvp {
    /// Pass `bytes` to `out`, with every `CSI f` final byte rewritten to `H`.
    ///
    /// The state is kept between calls: the `f` of a sequence split between two
    /// reads must be recognized as a final byte too.
    pub fn translate(&mut self, bytes: &[u8], out: &mut dyn FnMut(&[u8])) {
        if self.state == State::Ground && !bytes.contains(&0x1b) {
            out(bytes);
            return
        }

        let mut translated = Vec::with_capacity(bytes.len());

        for &byte in bytes {
            translated.push(self.step(byte));
        }

        out(&translated);
    }

    /// Translate one byte and return what the parser should see.
    fn step(&mut self, byte: u8) -> u8 {
        match self.state {
            State::Ground => {
                if byte == 0x1b {
                    self.state = State::Escape;
                }
            },

            State::Escape => {
                self.state = match byte {
                    b'[' => State::Csi,
                    b']' => State::Osc,
                    b'P' | b'X' | b'^' | b'_' => State::String,
                    0x1b => State::Escape,
                    _ => State::Ground,
                };
            },

            State::Csi => {
                match byte {
                    // A new escape aborts the sequence.
                    0x1b => self.state = State::Escape,
                    // The final byte: `f` is an alias of `H`.
                    0x40..=0x7e => {
                        self.state = State::Ground;

                        if byte == b'f' {
                            return b'H'
                        }
                    },
                    // Parameters and intermediates.
                    _ => (),
                }
            },

            State::Osc => {
                match byte {
                    0x07 => self.state = State::Ground,
                    0x1b => self.state = State::OscEscape,
                    _ => (),
                }
            },

            State::OscEscape => {
                self.state = match byte {
                    b'\\' => State::Ground,
                    0x1b => State::OscEscape,
                    _ => State::Osc,
                };
            },

            State::String => {
                if byte == 0x1b {
                    self.state = State::StringEscape;
                }
            },

            State::StringEscape => {
                self.state = match byte {
                    b'\\' => State::Ground,
                    0x1b => State::StringEscape,
                    _ => State::String,
                };
            },
        }

        byte
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn translate(hvp: &mut Hvp, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        hvp.translate(bytes, &mut |translated| out.extend_from_slice(translated));
        out
    }

    #[test]
    fn test_hvp_becomes_cup() {
        let mut hvp = Hvp::default();
        assert_eq!(
            translate(&mut hvp, b"\x1b[999;999f"),
            b"\x1b[999;999H".to_vec()
        );
    }

    /// The `f` of a sequence split between two reads is still a final byte.
    #[test]
    fn test_split_hvp() {
        let mut hvp = Hvp::default();

        assert_eq!(translate(&mut hvp, b"\x1b[3;5"), b"\x1b[3;5".to_vec());
        assert_eq!(translate(&mut hvp, b"f"), b"H".to_vec());
    }

    /// A plain `f` in the text is not touched, and neither is the `f` of any
    /// other sequence's final byte.
    #[test]
    fn test_plain_text_is_untouched() {
        let mut hvp = Hvp::default();
        assert_eq!(translate(&mut hvp, b"foo"), b"foo".to_vec());

        let mut hvp = Hvp::default();
        assert_eq!(
            translate(&mut hvp, b"\x1b[1mfoo\x1b[0m"),
            b"\x1b[1mfoo\x1b[0m".to_vec()
        );
    }

    /// A printable string may contain an `f`; it is data, not a final byte.
    #[test]
    fn test_string_payloads_are_untouched() {
        let mut hvp = Hvp::default();
        assert_eq!(
            translate(&mut hvp, b"\x1b]0;a f title\x07"),
            b"\x1b]0;a f title\x07".to_vec()
        );

        let mut hvp = Hvp::default();
        assert_eq!(
            translate(&mut hvp, b"\x1bP1;2|f data\x1b\\"),
            b"\x1bP1;2|f data\x1b\\".to_vec()
        );
    }

    /// An aborted CSI does not leak into the next one.
    #[test]
    fn test_escape_restarts_the_sequence() {
        let mut hvp = Hvp::default();
        assert_eq!(
            translate(&mut hvp, b"\x1b[1\x1b[4;2f"),
            b"\x1b[1\x1b[4;2H".to_vec()
        );
    }

    /// After a final byte the machine is back to plain text.
    #[test]
    fn test_state_resets_after_the_final_byte() {
        let mut hvp = Hvp::default();
        assert_eq!(translate(&mut hvp, b"\x1b[2;3Hf"), b"\x1b[2;3Hf".to_vec());
    }
}
