// input — turning raw bytes from the terminal into keys, escape sequences included.

use crate::tty::{poll, read_byte, PollFd, POLLIN, STDIN};

#[derive(PartialEq, Clone, Copy)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Home,
    End,
    PageUp,
    PageDown,
    WordLeft,
    WordRight,
    CtrlA,
    CtrlB,
    CtrlE,
    CtrlF,
    CtrlG,
    CtrlK,
    CtrlQ,
    CtrlS,
    CtrlU,
    CtrlW,
    Esc,
}

pub fn read_key() -> Option<Key> {
    let c = read_byte()?;
    match c {
        27 => {
            // a real escape sequence sends its bytes back-to-back; poll briefly
            // so a lone Esc doesn't block waiting for bytes that never come
            let mut pfd = PollFd { fd: STDIN, events: POLLIN, revents: 0 };
            if unsafe { poll(&mut pfd, 1, 25) } <= 0 {
                return Some(Key::Esc);
            }
            if read_byte() != Some(b'[') {
                return Some(Key::Esc);
            }
            // CSI sequences carry parameter bytes (digits, ';') before a final
            // byte >= 0x40 — parse them so ctrl/alt+arrow are distinguishable
            let mut params: Vec<u8> = Vec::new();
            let mut b = read_byte();
            while let Some(p) = b {
                if p < 0x40 {
                    params.push(p);
                    b = read_byte();
                } else {
                    break;
                }
            }
            match b {
                Some(b'A') => Some(Key::ArrowUp),
                Some(b'B') => Some(Key::ArrowDown),
                Some(b'C') => {
                    if params.contains(&b'3') || params.contains(&b'5') {
                        Some(Key::WordRight)
                    } else {
                        Some(Key::ArrowRight)
                    }
                }
                Some(b'D') => {
                    if params.contains(&b'3') || params.contains(&b'5') {
                        Some(Key::WordLeft)
                    } else {
                        Some(Key::ArrowLeft)
                    }
                }
                Some(b'H') => Some(Key::Home),
                Some(b'F') => Some(Key::End),
                Some(b'~') => match params.as_slice() {
                    [b'3'] => Some(Key::Delete),
                    [b'5'] => Some(Key::PageUp),
                    [b'6'] => Some(Key::PageDown),
                    [b'1'] | [b'7'] => Some(Key::Home),
                    [b'4'] | [b'8'] => Some(Key::End),
                    _ => Some(Key::Esc),
                },
                _ => Some(Key::Esc),
            }
        }
        8 | 127 => Some(Key::Backspace),
        13 => Some(Key::Enter),
        9 => Some(Key::Tab),
        v if v < 32 => match v {
            1 => Some(Key::CtrlA),
            2 => Some(Key::CtrlB),
            5 => Some(Key::CtrlE),
            6 => Some(Key::CtrlF),
            7 => Some(Key::CtrlG),
            11 => Some(Key::CtrlK),
            17 => Some(Key::CtrlQ),
            19 => Some(Key::CtrlS),
            21 => Some(Key::CtrlU),
            23 => Some(Key::CtrlW),
            _ => Some(Key::Esc),    // Ctrl-C and friends are no-ops
        },
        v => Some(Key::Char(v as char)),
    }
}