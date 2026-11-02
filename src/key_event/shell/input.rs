// Input handling for the attached shell view.
//
// The shell owns every key except the prefix chord. Keys are translated back
// into the byte sequences a terminal emulator would send to the child.

use anyhow::{anyhow, bail};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key together with its modifiers.
///
/// Parsed from the same specs the `shell_prefix` option uses: `C-s`, `M-u`,
/// `C-M-s`, `F12`, `PageUp`, `b`, … The keymap's shell sections use this so
/// that chords such as `Alt+u` can be bound from `keymap.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeySpec {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeySpec {
    /// Parse a spec such as `C-s`, `M-x`, `C-M-s`, `F12` or `b`.
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let spec = spec.trim();

        if spec.is_empty() {
            bail!("empty key spec")
        }

        let mut parts = spec.split('-').collect::<Vec<_>>();
        let key_part = parts.pop().expect("The spec is not empty");
        let mut modifiers = KeyModifiers::empty();

        for part in parts {
            match part.to_ascii_uppercase().as_str() {
                "C" | "CTRL" => modifiers |= KeyModifiers::CONTROL,
                "M" | "ALT" => modifiers |= KeyModifiers::ALT,
                "S" | "SHIFT" => modifiers |= KeyModifiers::SHIFT,
                _ => bail!("unknown modifier `{part}`"),
            }
        }

        let code = match key_part.to_ascii_lowercase().as_str() {
            "enter" | "return" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "esc" | "escape" => KeyCode::Esc,
            "space" => KeyCode::Char(' '),
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "insert" => KeyCode::Insert,
            "delete" => KeyCode::Delete,
            "f1" => KeyCode::F(1),
            "f2" => KeyCode::F(2),
            "f3" => KeyCode::F(3),
            "f4" => KeyCode::F(4),
            "f5" => KeyCode::F(5),
            "f6" => KeyCode::F(6),
            "f7" => KeyCode::F(7),
            "f8" => KeyCode::F(8),
            "f9" => KeyCode::F(9),
            "f10" => KeyCode::F(10),
            "f11" => KeyCode::F(11),
            "f12" => KeyCode::F(12),
            _ => {
                let mut chars = key_part.chars();
                let key = match chars.next() {
                    Some(key) => key,
                    None => bail!("key spec has no key"),
                };

                if chars.next().is_some() {
                    bail!("unknown key `{key_part}`")
                }

                KeyCode::Char(key)
            },
        };

        // A terminal reports `G` as `Char('G')` + SHIFT; an uppercase spec has
        // to mean the same thing, otherwise the binding could never match.
        if let KeyCode::Char(c) = code
            && c.is_uppercase()
            && !modifiers.contains(KeyModifiers::SHIFT)
        {
            modifiers |= KeyModifiers::SHIFT;
        }

        Ok(Self { code, modifiers })
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        event.code == self.code && event.modifiers == self.modifiers
    }

    /// The spec in the short form the status line shows, e.g. `M-u`, `PgUp`.
    pub fn label(&self) -> String {
        let mut label = String::new();
        // An uppercase character already implies SHIFT: `S-G` is just `G`.
        let shift_implied = matches!(self.code, KeyCode::Char(c) if c.is_uppercase());

        if self.modifiers.contains(KeyModifiers::CONTROL) {
            label.push_str("C-");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            label.push_str("M-");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) && !shift_implied {
            label.push_str("S-");
        }

        let key = match self.code {
            KeyCode::Char(' ') => String::from("Space"),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => String::from("Enter"),
            KeyCode::Tab => String::from("Tab"),
            KeyCode::BackTab => String::from("BackTab"),
            KeyCode::Esc => String::from("Esc"),
            KeyCode::Up => String::from("Up"),
            KeyCode::Down => String::from("Down"),
            KeyCode::Left => String::from("Left"),
            KeyCode::Right => String::from("Right"),
            KeyCode::Home => String::from("Home"),
            KeyCode::End => String::from("End"),
            KeyCode::PageUp => String::from("PgUp"),
            KeyCode::PageDown => String::from("PgDn"),
            KeyCode::Insert => String::from("Ins"),
            KeyCode::Delete => String::from("Del"),
            KeyCode::F(n) => format!("F{n}"),
            _ => String::from("?"),
        };

        label.push_str(&key);
        label
    }
}

/// The chord which gives hire a command while a shell is attached.
#[derive(Clone, Copy, Debug)]
pub struct ShellPrefix(KeySpec);

impl Default for ShellPrefix {
    fn default() -> Self {
        Self(KeySpec {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
        })
    }
}

impl ShellPrefix {
    /// Parse a spec such as `C-s`, `M-x`, `C-M-s`, `F12` or `b`.
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        KeySpec::parse(spec)
            .map(Self)
            .map_err(|err| anyhow!("invalid `shell_prefix`: {err}"))
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.0.matches(event)
    }
}

/// Translate a crossterm key event into the bytes a terminal sends.
///
/// `application_cursor` is the child's DECCKM state: curses programs switch it
/// on and then expect `ESC O A` instead of `ESC [ A` for the arrow keys.
/// `None` means the key cannot be represented; it is ignored instead of
/// silently sending something wrong.
pub fn encode_key(key: &KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let mut bytes = Vec::new();

    if alt {
        bytes.push(0x1b);
    }

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                bytes.push(ctrl_byte(c)?);
            } else {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        },

        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::BackTab => bytes.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => bytes.push(if ctrl { 0x08 } else { 0x7f }),
        KeyCode::Esc => bytes.push(0x1b),

        KeyCode::Up => bytes.extend_from_slice(
            if application_cursor { b"\x1bOA" } else { b"\x1b[A" }
        ),
        KeyCode::Down => bytes.extend_from_slice(
            if application_cursor { b"\x1bOB" } else { b"\x1b[B" }
        ),
        KeyCode::Right => bytes.extend_from_slice(
            if application_cursor { b"\x1bOC" } else { b"\x1b[C" }
        ),
        KeyCode::Left => bytes.extend_from_slice(
            if application_cursor { b"\x1bOD" } else { b"\x1b[D" }
        ),

        KeyCode::Home => bytes.extend_from_slice(
            if application_cursor { b"\x1bOH" } else { b"\x1b[H" }
        ),
        KeyCode::End => bytes.extend_from_slice(
            if application_cursor { b"\x1bOF" } else { b"\x1b[F" }
        ),
        KeyCode::PageUp => bytes.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => bytes.extend_from_slice(b"\x1b[6~"),
        KeyCode::Insert => bytes.extend_from_slice(b"\x1b[2~"),
        KeyCode::Delete => bytes.extend_from_slice(b"\x1b[3~"),

        KeyCode::F(1) => bytes.extend_from_slice(b"\x1bOP"),
        KeyCode::F(2) => bytes.extend_from_slice(b"\x1bOQ"),
        KeyCode::F(3) => bytes.extend_from_slice(b"\x1bOR"),
        KeyCode::F(4) => bytes.extend_from_slice(b"\x1bOS"),
        KeyCode::F(5) => bytes.extend_from_slice(b"\x1b[15~"),
        KeyCode::F(6) => bytes.extend_from_slice(b"\x1b[17~"),
        KeyCode::F(7) => bytes.extend_from_slice(b"\x1b[18~"),
        KeyCode::F(8) => bytes.extend_from_slice(b"\x1b[19~"),
        KeyCode::F(9) => bytes.extend_from_slice(b"\x1b[20~"),
        KeyCode::F(10) => bytes.extend_from_slice(b"\x1b[21~"),
        KeyCode::F(11) => bytes.extend_from_slice(b"\x1b[23~"),
        KeyCode::F(12) => bytes.extend_from_slice(b"\x1b[24~"),
        KeyCode::F(13) => bytes.extend_from_slice(b"\x1b[25~"),
        KeyCode::F(14) => bytes.extend_from_slice(b"\x1b[26~"),
        KeyCode::F(15) => bytes.extend_from_slice(b"\x1b[28~"),
        KeyCode::F(16) => bytes.extend_from_slice(b"\x1b[29~"),
        KeyCode::F(17) => bytes.extend_from_slice(b"\x1b[31~"),
        KeyCode::F(18) => bytes.extend_from_slice(b"\x1b[32~"),
        KeyCode::F(19) => bytes.extend_from_slice(b"\x1b[33~"),
        KeyCode::F(20) => bytes.extend_from_slice(b"\x1b[34~"),

        KeyCode::Null => bytes.push(0x00),

        _ => return None,
    }

    Some(bytes)
}

/// Encode pasted text for the child.
///
/// With bracketed paste the text is wrapped in `ESC[200~` / `ESC[201~` and sent
/// verbatim. Without it the paste is indistinguishable from typing, so line
/// breaks become `cr`; an embedded `\n` is not a terminal Enter.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let mut bytes = Vec::new();

    if bracketed {
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        return bytes
    }

    let normalized = text.replace("\r\n", "\r");

    for ch in normalized.chars() {
        if ch == '\n' {
            bytes.push(b'\r');
            continue
        }

        let mut buf = [0u8; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }

    bytes
}

/// The byte a `Ctrl-<char>` combination produces.
fn ctrl_byte(c: char) -> Option<u8> {
    match c {
        ' ' | '@' => Some(0x00),
        'a'..='z' => Some(c as u8 & 0x1f),
        'A'..='Z' => Some(c as u8 & 0x1f),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_arrows_follow_application_cursor() {
        let up = key(KeyCode::Up, KeyModifiers::NONE);

        assert_eq!(encode_key(&up, false).unwrap(), b"\x1b[A");
        assert_eq!(encode_key(&up, true).unwrap(), b"\x1bOA");

        let home = key(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(encode_key(&home, false).unwrap(), b"\x1b[H");
        assert_eq!(encode_key(&home, true).unwrap(), b"\x1bOH");
    }

    #[test]
    fn test_paste_encoding() {
        assert_eq!(encode_paste("a\nb", false), b"a\rb".to_vec());
        assert_eq!(encode_paste("a\r\nb", false), b"a\rb".to_vec());
        assert_eq!(
            encode_paste("a\nb", true),
            b"\x1b[200~a\nb\x1b[201~".to_vec()
        );

        // CJK text must keep its UTF-8 bytes.
        assert_eq!(
            encode_paste("中文", false),
            "中文".as_bytes().to_vec()
        );
    }

    #[test]
    fn test_ctrl_s_byte() {
        let ctrl_s = key(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(encode_key(&ctrl_s, false).unwrap(), vec![0x13]);
    }

    #[test]
    fn test_key_spec_parse_and_label() {
        let alt_u = KeySpec::parse("M-u").unwrap();
        assert!(alt_u.matches(&key(KeyCode::Char('u'), KeyModifiers::ALT)));
        assert!(!alt_u.matches(&key(KeyCode::Char('u'), KeyModifiers::empty())));
        assert_eq!(alt_u.label(), "M-u");

        // An uppercase spec carries the SHIFT the terminal reports.
        let shift_g = KeySpec::parse("G").unwrap();
        assert!(shift_g.matches(&key(KeyCode::Char('G'), KeyModifiers::SHIFT)));
        assert!(!shift_g.matches(&key(KeyCode::Char('g'), KeyModifiers::empty())));
        assert_eq!(shift_g.label(), "G");

        assert_eq!(KeySpec::parse("PageUp").unwrap().label(), "PgUp");
        assert_eq!(KeySpec::parse("C-b").unwrap().label(), "C-b");
        assert_eq!(KeySpec::parse("Esc").unwrap().label(), "Esc");

        assert!(KeySpec::parse("").is_err());
        assert!(KeySpec::parse("C-").is_err());
        assert!(KeySpec::parse("C-Unknown").is_err());
    }

    #[test]
    fn test_parse_and_match_prefix() {
        let prefix = ShellPrefix::parse("C-s").unwrap();
        assert!(prefix.matches(&key(KeyCode::Char('s'), KeyModifiers::CONTROL)));
        assert!(!prefix.matches(&key(KeyCode::Char('b'), KeyModifiers::CONTROL)));

        assert!(ShellPrefix::parse("").is_err());
        assert!(ShellPrefix::parse("C-").is_err());
    }
}
