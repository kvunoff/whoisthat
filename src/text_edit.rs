use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub(crate) fn edit_text_field(s: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(c) if c == 'v' && matches!(key.modifiers, KeyModifiers::CONTROL) => {
            if let Some(clip) = read_clipboard() {
                *s = clip;
                *cursor = s.len();
            }
            false
        }
        KeyCode::Char(c) => {
            if *cursor <= s.len() {
                s.insert(*cursor, c);
            } else {
                s.push(c);
            }
            *cursor += 1;
            false
        }
        KeyCode::Backspace => {
            if *cursor > 0 && !s.is_empty() {
                s.remove(*cursor - 1);
                *cursor -= 1;
            }
            false
        }
        KeyCode::Delete => {
            if *cursor < s.len() {
                s.remove(*cursor);
            }
            false
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            false
        }
        KeyCode::Right => {
            if *cursor < s.len() {
                *cursor += 1;
            }
            false
        }
        KeyCode::Home => {
            *cursor = 0;
            false
        }
        KeyCode::End => {
            *cursor = s.len();
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn insert_char_at_end() {
        let mut s = String::from("ab");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('c')));
        assert_eq!(s, "abc");
        assert_eq!(c, 3);
    }

    #[test]
    fn insert_char_at_start() {
        let mut s = String::from("bc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('a')));
        assert_eq!(s, "abc");
        assert_eq!(c, 1);
    }

    #[test]
    fn insert_char_in_middle() {
        let mut s = String::from("ac");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('b')));
        assert_eq!(s, "abc");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_removes_char() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "ab");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "abc");
        assert_eq!(c, 0);
    }

    #[test]
    fn backspace_in_middle() {
        let mut s = String::from("abc");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "ac");
        assert_eq!(c, 1);
    }

    #[test]
    fn delete_removes_char_at_cursor() {
        let mut s = String::from("abc");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Delete));
        assert_eq!(s, "ac");
        assert_eq!(c, 1);
    }

    #[test]
    fn delete_at_end_is_noop() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Delete));
        assert_eq!(s, "abc");
        assert_eq!(c, 3);
    }

    #[test]
    fn left_moves_cursor() {
        let mut s = String::from("abc");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Left));
        assert_eq!(c, 1);
        assert_eq!(s, "abc");
    }

    #[test]
    fn left_at_start_is_noop() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Left));
        assert_eq!(c, 0);
    }

    #[test]
    fn right_moves_cursor() {
        let mut s = String::from("abc");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Right));
        assert_eq!(c, 2);
    }

    #[test]
    fn right_at_end_is_noop() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Right));
        assert_eq!(c, 3);
    }

    #[test]
    fn home_moves_to_start() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Home));
        assert_eq!(c, 0);
    }

    #[test]
    fn end_moves_to_end() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::End));
        assert_eq!(c, 3);
    }

    #[test]
    fn ctrl_v_is_not_insert() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key_ctrl(KeyCode::Char('v')));
        assert!(c <= s.len());
    }
}
