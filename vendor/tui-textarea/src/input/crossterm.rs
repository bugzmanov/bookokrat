use super::{Input, Key};
use crate::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};

impl From<Event> for Input {
    /// Convert [`crossterm::event::Event`] into [`Input`].
    fn from(event: Event) -> Self {
        match event {
            Event::Key(key) => Self::from(key),
            Event::Mouse(mouse) => Self::from(mouse),
            _ => Self::default(),
        }
    }
}

impl From<KeyCode> for Key {
    /// Convert [`crossterm::event::KeyCode`] into [`Key`].
    fn from(code: KeyCode) -> Self {
        match code {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Enter => Key::Enter,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Tab => Key::Tab,
            KeyCode::Delete => Key::Delete,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::Esc => Key::Esc,
            KeyCode::F(x) => Key::F(x),
            _ => Key::Null,
        }
    }
}

impl From<KeyEvent> for Input {
    /// Convert [`crossterm::event::KeyEvent`] into [`Input`].
    fn from(key: KeyEvent) -> Self {
        if key.kind == KeyEventKind::Release {
            // On Windows or when `crossterm::event::PushKeyboardEnhancementFlags` is set,
            // key release event can be reported. Ignore it. (#14)
            return Self::default();
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let key = Key::from(key.code);

        Self {
            key,
            ctrl,
            alt,
            shift,
        }
    }
}

impl From<MouseEventKind> for Key {
    /// Convert [`crossterm::event::MouseEventKind`] into [`Key`].
    fn from(kind: MouseEventKind) -> Self {
        match kind {
            MouseEventKind::ScrollDown => Key::MouseScrollDown,
            MouseEventKind::ScrollUp => Key::MouseScrollUp,
            _ => Key::Null,
        }
    }
}

impl From<MouseEvent> for Input {
    /// Convert [`crossterm::event::MouseEvent`] into [`Input`].
    fn from(mouse: MouseEvent) -> Self {
        let key = Key::from(mouse.kind);
        let ctrl = mouse.modifiers.contains(KeyModifiers::CONTROL);
        let alt = mouse.modifiers.contains(KeyModifiers::ALT);
        let shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
        Self {
            key,
            ctrl,
            alt,
            shift,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossterm::event::KeyEventState;
    use crate::input::tests::input;

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn mouse_event(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
        MouseEvent {
            kind,
            column: 1,
            row: 1,
            modifiers,
        }
    }

    #[test]
    fn key_to_input() {
        for (from, to) in [
            (
                key_event(KeyCode::Char('a'), KeyModifiers::empty()),
                input(Key::Char('a'), false, false, false),
            ),
            (
                key_event(KeyCode::Enter, KeyModifiers::empty()),
                input(Key::Enter, false, false, false),
            ),
            (
                key_event(KeyCode::Left, KeyModifiers::CONTROL),
                input(Key::Left, true, false, false),
            ),
            (
                key_event(KeyCode::Right, KeyModifiers::SHIFT),
                input(Key::Right, false, false, true),
            ),
            (
                key_event(KeyCode::Home, KeyModifiers::ALT),
                input(Key::Home, false, true, false),
            ),
            (
                key_event(
                    KeyCode::F(1),
                    KeyModifiers::ALT | KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                ),
                input(Key::F(1), true, true, true),
            ),
            (
                key_event(KeyCode::NumLock, KeyModifiers::CONTROL),
                input(Key::Null, true, false, false),
            ),
        ] {
            assert_eq!(Input::from(from), to, "{:?} -> {:?}", from, to);
        }
    }

    #[test]
    fn mouse_to_input() {
        for (from, to) in [
            (
                mouse_event(MouseEventKind::ScrollDown, KeyModifiers::empty()),
                input(Key::MouseScrollDown, false, false, false),
            ),
            (
                mouse_event(MouseEventKind::ScrollUp, KeyModifiers::CONTROL),
                input(Key::MouseScrollUp, true, false, false),
            ),
            (
                mouse_event(MouseEventKind::ScrollUp, KeyModifiers::SHIFT),
                input(Key::MouseScrollUp, false, false, true),
            ),
            (
                mouse_event(MouseEventKind::ScrollDown, KeyModifiers::ALT),
                input(Key::MouseScrollDown, false, true, false),
            ),
            (
                mouse_event(
                    MouseEventKind::ScrollUp,
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                ),
                input(Key::MouseScrollUp, true, true, false),
            ),
            (
                mouse_event(MouseEventKind::Moved, KeyModifiers::CONTROL),
                input(Key::Null, true, false, false),
            ),
        ] {
            assert_eq!(Input::from(from), to, "{:?} -> {:?}", from, to);
        }
    }

    #[test]
    fn event_to_input() {
        for (from, to) in [
            (
                Event::Key(key_event(KeyCode::Char('a'), KeyModifiers::empty())),
                input(Key::Char('a'), false, false, false),
            ),
            (
                Event::Mouse(mouse_event(
                    MouseEventKind::ScrollDown,
                    KeyModifiers::empty(),
                )),
                input(Key::MouseScrollDown, false, false, false),
            ),
            (Event::FocusGained, input(Key::Null, false, false, false)),
        ] {
            assert_eq!(Input::from(from.clone()), to, "{:?} -> {:?}", from, to);
        }
    }

    // Regression for https://github.com/rhysd/tui-textarea/issues/14
    #[test]
    fn ignore_key_release_event() {
        let mut from = key_event(KeyCode::Char('a'), KeyModifiers::empty());
        from.kind = KeyEventKind::Release;
        let to = input(Key::Null, false, false, false);
        assert_eq!(Input::from(from), to, "{:?} -> {:?}", from, to);
    }
}
