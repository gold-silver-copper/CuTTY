use winit::{
    event::KeyEvent,
    keyboard::{Key, ModifiersState, NamedKey},
};

pub fn bytes_for_key_event(
    event: &KeyEvent,
    modifiers: ModifiersState,
    application_cursor: bool,
) -> Option<Vec<u8>> {
    let mut bytes = if modifiers.control_key() && !modifiers.alt_key() {
        ctrl_key_bytes(&event.logical_key)
    } else if let Key::Named(named) = event.logical_key.as_ref() {
        named_key_bytes(&named, application_cursor)
    } else {
        event.text.as_ref().map(|text| text.as_bytes().to_vec())
    }?;

    if modifiers.alt_key() && !matches!(event.logical_key.as_ref(), Key::Named(_)) {
        let mut prefixed = vec![0x1b];
        prefixed.append(&mut bytes);
        Some(prefixed)
    } else {
        Some(bytes)
    }
}

fn ctrl_key_bytes(key: &Key) -> Option<Vec<u8>> {
    if let Key::Character(ch) = key {
        let c = ch.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]);
        }
    }
    None
}

fn named_key_bytes(key: &NamedKey, application_cursor: bool) -> Option<Vec<u8>> {
    #[rustfmt::skip]
    let seq: &[u8] = match key {
        NamedKey::Enter      => b"\r",
        NamedKey::Backspace  => b"\x7f",
        NamedKey::Tab        => b"\t",
        NamedKey::Escape     => b"\x1b",
        NamedKey::Space      => b" ",
        NamedKey::ArrowUp    => if application_cursor { b"\x1bOA" } else { b"\x1b[A" },
        NamedKey::ArrowDown  => if application_cursor { b"\x1bOB" } else { b"\x1b[B" },
        NamedKey::ArrowRight => if application_cursor { b"\x1bOC" } else { b"\x1b[C" },
        NamedKey::ArrowLeft  => if application_cursor { b"\x1bOD" } else { b"\x1b[D" },
        NamedKey::Home       => if application_cursor { b"\x1bOH" } else { b"\x1b[H" },
        NamedKey::End        => if application_cursor { b"\x1bOF" } else { b"\x1b[F" },
        NamedKey::PageUp     => b"\x1b[5~",
        NamedKey::PageDown   => b"\x1b[6~",
        NamedKey::Delete     => b"\x1b[3~",
        NamedKey::Insert     => b"\x1b[2~",
        NamedKey::F1         => b"\x1bOP",
        NamedKey::F2         => b"\x1bOQ",
        NamedKey::F3         => b"\x1bOR",
        NamedKey::F4         => b"\x1bOS",
        NamedKey::F5         => b"\x1b[15~",
        NamedKey::F6         => b"\x1b[17~",
        NamedKey::F7         => b"\x1b[18~",
        NamedKey::F8         => b"\x1b[19~",
        NamedKey::F9         => b"\x1b[20~",
        NamedKey::F10        => b"\x1b[21~",
        NamedKey::F11        => b"\x1b[23~",
        NamedKey::F12        => b"\x1b[24~",
        _ => return None,
    };
    Some(seq.to_vec())
}
