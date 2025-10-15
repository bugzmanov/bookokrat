use crossterm::event::{KeyCode, KeyModifiers};
use tui_textarea::{Input, Key as TextAreaKey};

pub fn map_keys_to_input(key: crossterm::event::KeyEvent) -> Option<Input> {
    // Convert crossterm key to tui_textarea input
    let textarea_input = match key.code {
        KeyCode::Char(c) => Input {
            key: TextAreaKey::Char(c),
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Backspace => Input {
            key: TextAreaKey::Backspace,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Enter => Input {
            key: TextAreaKey::Enter,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Left => Input {
            key: TextAreaKey::Left,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Right => Input {
            key: TextAreaKey::Right,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Up => Input {
            key: TextAreaKey::Up,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Down => Input {
            key: TextAreaKey::Down,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Tab => Input {
            key: TextAreaKey::Tab,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Delete => Input {
            key: TextAreaKey::Delete,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Home => Input {
            key: TextAreaKey::Home,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::End => Input {
            key: TextAreaKey::End,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::PageUp => Input {
            key: TextAreaKey::PageUp,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::PageDown => Input {
            key: TextAreaKey::PageDown,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        KeyCode::Esc => Input {
            key: TextAreaKey::Esc,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        },
        _ => return None, // Ignore other keys
    };
    Some(textarea_input)
}
