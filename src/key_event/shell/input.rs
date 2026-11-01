// Input handling for the attached shell view.
//
// The shell owns every key except the prefix chord. Keys are translated back
// into the byte sequences a terminal emulator would send to the child.

use anyhow::bail;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The chord which gives hire a command while a shell is attached.
#[derive(Clone, Copy, Debug)]
pub struct ShellPrefix {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Default for ShellPrefix {
    fn default() -> Self {
        Self {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
        }
    }
}

impl ShellPrefix {
    /// Parse a spec such as `C-s`, `M-x`, `C-M-s`, `F12` or `b`.
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let spec = spec.trim();

        if spec.is_empty() {
            bail!("`shell_prefix` is empty")
        }

        let mut parts = spec.split('-').collect::<Vec<_>>();
        let key_part = parts.pop().expect("The spec is not empty");
        let mut modifiers = KeyModifiers::empty();

        for part in parts {
            match part.to_ascii_uppercase().as_str() {
                "C" | "CTRL" => modifiers |= KeyModifiers::CONTROL,
                "M" | "ALT" => modifiers |= KeyModifiers::ALT,
                "S" | "SHIFT" => modifiers |= KeyModifiers::SHIFT,
                _ => bail!("Unknown modifier `{part}` in `shell_prefix`"),
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
                    None => bail!("`shell_prefix` has no key"),
                };

                if chars.next().is_some() {
                    bail!("Unknown key `{key_part}` in `shell_prefix`")
                }

                KeyCode::Char(key)
            },
        };

        Ok(Self { code, modifiers })
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        event.code == self.code && event.modifiers == self.modifiers
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
    fn test_ctrl_s_byte() {
        let ctrl_s = key(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(encode_key(&ctrl_s, false).unwrap(), vec![0x13]);
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
