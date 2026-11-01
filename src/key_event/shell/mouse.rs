// Mouse event encoding for attached shell sessions.
//
// The outer terminal is captured with SGR mouse reporting; each event is
// re-encoded into the protocol mode and encoding the child requested.

use ratatui::{
    crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
};
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

/// The bit used to remember that a button is held down.
pub fn button_bit(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 4,
    }
}

/// Whether (`column`, `row`) lies inside `area` (0-based terminal cells).
pub fn inside(area: Rect, column: u16, row: u16) -> bool {
    area.width > 0 &&
        area.height > 0 &&
        column >= area.x &&
        column < area.x.saturating_add(area.width) &&
        row >= area.y &&
        row < area.y.saturating_add(area.height)
}

/// Encode `event` for the child's mouse protocol.
///
/// The caller has already checked the shell area and the pressed-button state:
/// a press must be inside the area; a drag/release outside is forwarded with
/// its coordinates clamped so a drag which leaves the shell area does not get
/// stuck.
pub fn encode_mouse(
    event: &MouseEvent,
    mode: MouseProtocolMode,
    encoding: MouseProtocolEncoding,
    area: Rect,
) -> Option<Vec<u8>>
{
    if area.width == 0 || area.height == 0 {
        return None
    }

    let kind = event.kind;
    let moving = matches!(kind, MouseEventKind::Drag(_) | MouseEventKind::Moved);
    let releasing = matches!(kind, MouseEventKind::Up(_));

    match mode {
        MouseProtocolMode::None => return None,

        // X10 mode: button presses and wheel only.
        MouseProtocolMode::Press => {
            if !matches!(
                kind,
                MouseEventKind::Down(_)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            ) {
                return None
            }
        },

        // VT200 mode: presses and releases, no motion.
        MouseProtocolMode::PressRelease => {
            if moving {
                return None
            }
        },

        // Button motion: no motion with no button held.
        MouseProtocolMode::ButtonMotion => {
            if matches!(kind, MouseEventKind::Moved) {
                return None
            }
        },

        MouseProtocolMode::AnyMotion => (),
    }

    let max_col = area.x.saturating_add(area.width - 1);
    let max_row = area.y.saturating_add(area.height - 1);

    if !inside(area, event.column, event.row) && !(releasing || moving) {
        return None
    }

    // The child's screen starts at the top-left corner of `area` and mouse
    // coordinates are 1-based.
    let column = event.column.clamp(area.x, max_col) - area.x + 1;
    let row = event.row.clamp(area.y, max_row) - area.y + 1;

    let mut button = match kind {
        MouseEventKind::Down(button) | MouseEventKind::Up(button) => {
            button_code(button)
        },
        MouseEventKind::Drag(button) => button_code(button) + 32,
        MouseEventKind::Moved => 3 + 32,
        MouseEventKind::ScrollUp => 64,
        MouseEventKind::ScrollDown => 65,
        MouseEventKind::ScrollLeft => 66,
        MouseEventKind::ScrollRight => 67,
    };
    button += modifier_code(event.modifiers);

    match encoding {
        MouseProtocolEncoding::Sgr => {
            let suffix = if releasing { 'm' } else { 'M' };
            Some(
                format!("\x1b[<{button};{column};{row}{suffix}").into_bytes(),
            )
        },

        MouseProtocolEncoding::Default => {
            // X10 has no per-button release code; button 3 means "release".
            let code = if releasing {
                3 + modifier_code(event.modifiers)
            } else {
                button
            };

            Some(vec![
                0x1b,
                b'[',
                b'M',
                32 + code as u8,
                32 + column.min(223) as u8,
                32 + row.min(223) as u8,
            ])
        },

        MouseProtocolEncoding::Utf8 => {
            let code = if releasing {
                3 + modifier_code(event.modifiers)
            } else {
                button
            };
            let mut bytes = b"\x1b[M".to_vec();

            for value in [32 + code, 32 + i32::from(column), 32 + i32::from(row)] {
                let ch = char::from_u32(value as u32).unwrap_or('?');
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }

            Some(bytes)
        },
    }
}

fn button_code(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

fn modifier_code(modifiers: KeyModifiers) -> i32 {
    let mut code = 0;

    if modifiers.contains(KeyModifiers::SHIFT) {
        code += 4;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        code += 8;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        code += 16;
    }

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 1, 100, 30)
    }

    fn event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_sgr_press_and_release() {
        let press = event(MouseEventKind::Down(MouseButton::Left), 4, 3);

        assert_eq!(
            encode_mouse(&press, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<0;5;3M".to_vec())
        );

        let release = event(MouseEventKind::Up(MouseButton::Left), 4, 3);
        assert_eq!(
            encode_mouse(&release, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<0;5;3m".to_vec())
        );
    }

    #[test]
    fn test_sgr_motion_and_wheel() {
        let moved = event(MouseEventKind::Moved, 12, 4);
        assert_eq!(
            encode_mouse(&moved, MouseProtocolMode::AnyMotion, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<35;13;4M".to_vec())
        );

        let drag = event(MouseEventKind::Drag(MouseButton::Right), 12, 4);
        assert_eq!(
            encode_mouse(&drag, MouseProtocolMode::ButtonMotion, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<34;13;4M".to_vec())
        );

        let wheel = event(MouseEventKind::ScrollDown, 2, 2);
        assert_eq!(
            encode_mouse(&wheel, MouseProtocolMode::Press, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<65;3;2M".to_vec())
        );
    }

    #[test]
    fn test_default_and_utf8_encoding() {
        let press = event(MouseEventKind::Down(MouseButton::Left), 4, 3);

        assert_eq!(
            encode_mouse(&press, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Default, area()),
            Some(vec![0x1b, b'[', b'M', 32, 37, 35])
        );

        assert_eq!(
            encode_mouse(&press, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Utf8, area()),
            Some(b"\x1b[M %#".to_vec())
        );

        let release = event(MouseEventKind::Up(MouseButton::Left), 4, 3);
        assert_eq!(
            encode_mouse(&release, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Default, area()),
            Some(vec![0x1b, b'[', b'M', 35, 37, 35])
        );
    }

    #[test]
    fn test_mode_and_area_filtering() {
        // Press mode ignores releases.
        let release = event(MouseEventKind::Up(MouseButton::Left), 4, 3);
        assert_eq!(
            encode_mouse(&release, MouseProtocolMode::Press, MouseProtocolEncoding::Sgr, area()),
            None
        );

        // A press outside the shell area is not forwarded.
        let outside = event(MouseEventKind::Down(MouseButton::Left), 4, 0);
        assert_eq!(
            encode_mouse(&outside, MouseProtocolMode::AnyMotion, MouseProtocolEncoding::Sgr, area()),
            None
        );

        // A release outside is clamped to the edge of the shell area.
        let outside_release = event(MouseEventKind::Up(MouseButton::Left), 4, 0);
        assert_eq!(
            encode_mouse(&outside_release, MouseProtocolMode::PressRelease, MouseProtocolEncoding::Sgr, area()),
            Some(b"\x1b[<0;5;1m".to_vec())
        );
    }
}
