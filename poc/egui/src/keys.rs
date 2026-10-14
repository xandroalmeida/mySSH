//! Tradução de teclas do egui para bytes de terminal (xterm).

use egui::{Key, Modifiers};

fn csi_mod(m: Modifiers) -> u8 {
    // xterm: 1 + (shift=1, alt=2, ctrl=4)
    1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.ctrl as u8)
}

fn cursor_seq(final_byte: u8, m: Modifiers, app_cursor: bool) -> Vec<u8> {
    let md = csi_mod(m);
    if md > 1 {
        format!("\x1b[1;{}{}", md, final_byte as char).into_bytes()
    } else if app_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

fn tilde_seq(n: u8, m: Modifiers) -> Vec<u8> {
    let md = csi_mod(m);
    if md > 1 {
        format!("\x1b[{n};{md}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

/// Retorna os bytes a enviar para uma tecla pressionada, ou `None` se a tecla é
/// tratada como texto (`Event::Text`) ou não tem tradução.
pub fn encode_key(key: Key, m: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    // Ctrl+letra -> byte de controle (Ctrl+A = 0x01 ... Ctrl+Z = 0x1a).
    if m.ctrl {
        if let Some(c) = letter(key) {
            let b = (c as u8) & 0x1f;
            return Some(if m.alt { vec![0x1b, b] } else { vec![b] });
        }
        match key {
            Key::Space => return Some(vec![0]),
            Key::OpenBracket => return Some(vec![0x1b]),
            Key::Backslash => return Some(vec![0x1c]),
            Key::CloseBracket => return Some(vec![0x1d]),
            _ => {}
        }
    }
    let v = match key {
        Key::Enter => vec![b'\r'],
        Key::Tab => {
            if m.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Backspace => {
            if m.alt {
                vec![0x1b, 0x7f]
            } else {
                vec![0x7f]
            }
        }
        Key::Escape => vec![0x1b],
        Key::ArrowUp => cursor_seq(b'A', m, app_cursor),
        Key::ArrowDown => cursor_seq(b'B', m, app_cursor),
        Key::ArrowRight => cursor_seq(b'C', m, app_cursor),
        Key::ArrowLeft => cursor_seq(b'D', m, app_cursor),
        Key::Home => cursor_seq(b'H', m, app_cursor),
        Key::End => cursor_seq(b'F', m, app_cursor),
        Key::PageUp => tilde_seq(5, m),
        Key::PageDown => tilde_seq(6, m),
        Key::Insert => tilde_seq(2, m),
        Key::Delete => tilde_seq(3, m),
        Key::F1 => ss3_or_csi(b'P', m),
        Key::F2 => ss3_or_csi(b'Q', m),
        Key::F3 => ss3_or_csi(b'R', m),
        Key::F4 => ss3_or_csi(b'S', m),
        Key::F5 => tilde_seq(15, m),
        Key::F6 => tilde_seq(17, m),
        Key::F7 => tilde_seq(18, m),
        Key::F8 => tilde_seq(19, m),
        Key::F9 => tilde_seq(20, m),
        Key::F10 => tilde_seq(21, m),
        Key::F11 => tilde_seq(23, m),
        Key::F12 => tilde_seq(24, m),
        _ => return None,
    };
    Some(v)
}

fn ss3_or_csi(final_byte: u8, m: Modifiers) -> Vec<u8> {
    let md = csi_mod(m);
    if md > 1 {
        format!("\x1b[1;{}{}", md, final_byte as char).into_bytes()
    } else {
        vec![0x1b, b'O', final_byte]
    }
}

fn letter(key: Key) -> Option<char> {
    let n = key.name();
    let mut it = n.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// Prepara texto colado: remove ESC (impede encerrar o bracketed paste por dentro).
pub fn sanitize_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let clean: String = text.chars().filter(|c| *c != '\x1b').collect();
    // Terminais convertem \n em \r ao colar.
    let clean = clean.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let mut v = b"\x1b[200~".to_vec();
        v.extend_from_slice(clean.as_bytes());
        v.extend_from_slice(b"\x1b[201~");
        v
    } else {
        clean.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_letters() {
        assert_eq!(encode_key(Key::C, Modifiers::CTRL, false), Some(vec![3]));
        assert_eq!(encode_key(Key::A, Modifiers::CTRL, false), Some(vec![1]));
        assert_eq!(encode_key(Key::Z, Modifiers::CTRL, false), Some(vec![26]));
    }

    #[test]
    fn arrows_follow_app_cursor() {
        assert_eq!(
            encode_key(Key::ArrowUp, Modifiers::NONE, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_key(Key::ArrowUp, Modifiers::NONE, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode_key(Key::ArrowLeft, Modifiers::CTRL, true),
            Some(b"\x1b[1;5D".to_vec())
        );
    }

    #[test]
    fn basics() {
        assert_eq!(
            encode_key(Key::Enter, Modifiers::NONE, false),
            Some(vec![b'\r'])
        );
        assert_eq!(
            encode_key(Key::Tab, Modifiers::NONE, false),
            Some(vec![b'\t'])
        );
        assert_eq!(
            encode_key(Key::Backspace, Modifiers::NONE, false),
            Some(vec![0x7f])
        );
        assert_eq!(
            encode_key(Key::Escape, Modifiers::NONE, false),
            Some(vec![0x1b])
        );
        assert_eq!(
            encode_key(Key::F5, Modifiers::NONE, false),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            encode_key(Key::F1, Modifiers::NONE, false),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(encode_key(Key::A, Modifiers::NONE, false), None);
    }

    #[test]
    fn paste_is_sanitized() {
        assert_eq!(
            sanitize_paste("a\x1b[201~b\n", true),
            b"\x1b[200~a[201~b\r\x1b[201~".to_vec()
        );
        assert_eq!(sanitize_paste("x", false), b"x".to_vec());
    }
}
