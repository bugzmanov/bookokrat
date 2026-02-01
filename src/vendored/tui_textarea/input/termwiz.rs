use super::{Input, Key};
use termwiz::input::{
    InputEvent, KeyCode, KeyEvent, Modifiers, MouseButtons, MouseEvent, PixelMouseEvent,
};

impl From<InputEvent> for Input {
    /// Convert [`termwiz::input::InputEvent`] into [`Input`].
    fn from(input: InputEvent) -> Self {
        match input {
            InputEvent::Key(key) => Self::from(key),
            InputEvent::Mouse(mouse) => Self::from(mouse),
            InputEvent::PixelMouse(mouse) => Self::from(mouse),
            _ => Self::default(),
        }
    }
}

impl From<KeyCode> for Key {
    /// Convert [`termwiz::input::KeyCode`] into [`Key`].
    fn from(key: KeyCode) -> Self {
        match key {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Tab => Key::Tab,
            KeyCode::Enter => Key::Enter,
            KeyCode::Escape => Key::Esc,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::End => Key::End,
            KeyCode::Home => Key::Home,
            KeyCode::LeftArrow => Key::Left,
            KeyCode::RightArrow => Key::Right,
            KeyCode::UpArrow => Key::Up,
            KeyCode::DownArrow => Key::Down,
            KeyCode::Delete => Key::Delete,
            KeyCode::Function(x) => Key::F(x),
            KeyCode::Copy => Key::Copy,
            KeyCode::Cut => Key::Cut,
            KeyCode::Paste => Key::Paste,
            _ => Key::Null,
        }
    }
}

impl From<KeyEvent> for Input {
    /// Convert [`termwiz::input::KeyEvent`] into [`Input`].
    fn from(key: KeyEvent) -> Self {
        let KeyEvent { key, modifiers } = key;
        let key = Key::from(key);
        let ctrl = modifiers.contains(Modifiers::CTRL);
        let alt = modifiers.contains(Modifiers::ALT);
        let shift = modifiers.contains(Modifiers::SHIFT);

        Self {
            key,
            ctrl,
            alt,
            shift,
        }
    }
}

impl From<MouseButtons> for Key {
    /// Convert [`termwiz::input::MouseButtons`] into [`Key`].
    fn from(buttons: MouseButtons) -> Self {
        if buttons.contains(MouseButtons::VERT_WHEEL) {
            if buttons.contains(MouseButtons::WHEEL_POSITIVE) {
                Key::MouseScrollUp
            } else {
                Key::MouseScrollDown
            }
        } else {
            Key::Null
        }
    }
}

impl From<MouseEvent> for Input {
    /// Convert [`termwiz::input::MouseEvent`] into [`Input`].
    fn from(mouse: MouseEvent) -> Self {
        let MouseEvent {
            mouse_buttons,
            modifiers,
            ..
        } = mouse;
        let key = Key::from(mouse_buttons);
        let ctrl = modifiers.contains(Modifiers::CTRL);
        let alt = modifiers.contains(Modifiers::ALT);
        let shift = modifiers.contains(Modifiers::SHIFT);

        Self {
            key,
            ctrl,
            alt,
            shift,
        }
    }
}

impl From<PixelMouseEvent> for Input {
    /// Convert [`termwiz::input::PixelMouseEvent`] into [`Input`].
    fn from(mouse: PixelMouseEvent) -> Self {
        let PixelMouseEvent {
            mouse_buttons,
            modifiers,
            ..
        } = mouse;

        let key = Key::from(mouse_buttons);
        let ctrl = modifiers.contains(Modifiers::CTRL);
        let alt = modifiers.contains(Modifiers::ALT);
        let shift = modifiers.contains(Modifiers::SHIFT);

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
    use crate::input::tests::input;

    fn key_event(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
        KeyEvent { key, modifiers }
    }

    fn mouse_event(mouse_buttons: MouseButtons, modifiers: Modifiers) -> MouseEvent {
        MouseEvent {
            mouse_buttons,
            modifiers,
            x: 1,
            y: 1,
        }
    }

    fn pixel_mouse_event(mouse_buttons: MouseButtons, modifiers: Modifiers) -> PixelMouseEvent {
        PixelMouseEvent {
            mouse_buttons,
            modifiers,
            x_pixels: 1,
            y_pixels: 1,
        }
    }

    #[test]
    fn key_to_input() {
        for (from, to) in [
            (
                key_event(KeyCode::Char('a'), Modifiers::empty()),
                input(Key::Char('a'), false, false, false),
            ),
            (
                key_event(KeyCode::Enter, Modifiers::empty()),
                input(Key::Enter, false, false, false),
            ),
            (
                key_event(KeyCode::LeftArrow, Modifiers::CTRL),
                input(Key::Left, true, false, false),
            ),
            (
                key_event(KeyCode::RightArrow, Modifiers::SHIFT),
                input(Key::Right, false, false, true),
            ),
            (
                key_event(KeyCode::Home, Modifiers::ALT),
                input(Key::Home, false, true, false),
            ),
            (
                key_event(
                    KeyCode::Function(1),
                    Modifiers::ALT | Modifiers::CTRL | Modifiers::SHIFT,
                ),
                input(Key::F(1), true, true, true),
            ),
            (
                key_event(KeyCode::NumLock, Modifiers::CTRL),
                input(Key::Null, true, false, false),
            ),
        ] {
            assert_eq!(Input::from(from.clone()), to, "{:?} -> {:?}", from, to);
        }
    }

    #[test]
    fn mouse_to_input() {
        for (from, to) in [
            (
                mouse_event(MouseButtons::VERT_WHEEL, Modifiers::empty()),
                input(Key::MouseScrollDown, false, false, false),
            ),
            (
                mouse_event(
                    MouseButtons::VERT_WHEEL | MouseButtons::WHEEL_POSITIVE,
                    Modifiers::empty(),
                ),
                input(Key::MouseScrollUp, false, false, false),
            ),
            (
                mouse_event(MouseButtons::VERT_WHEEL, Modifiers::CTRL),
                input(Key::MouseScrollDown, true, false, false),
            ),
            (
                mouse_event(MouseButtons::VERT_WHEEL, Modifiers::SHIFT),
                input(Key::MouseScrollDown, false, false, true),
            ),
            (
                mouse_event(MouseButtons::VERT_WHEEL, Modifiers::ALT),
                input(Key::MouseScrollDown, false, true, false),
            ),
            (
                mouse_event(
                    MouseButtons::VERT_WHEEL,
                    Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
                ),
                input(Key::MouseScrollDown, true, true, true),
            ),
            (
                mouse_event(MouseButtons::LEFT, Modifiers::empty()),
                input(Key::Null, false, false, false),
            ),
        ] {
            assert_eq!(Input::from(from.clone()), to, "{:?} -> {:?}", from, to);

            let from = pixel_mouse_event(from.mouse_buttons, from.modifiers);
            assert_eq!(Input::from(from.clone()), to, "{:?} -> {:?}", from, to);
        }
    }

    #[test]
    fn event_to_input() {
        for (from, to) in [
            (
                InputEvent::Key(key_event(KeyCode::Char('a'), Modifiers::empty())),
                input(Key::Char('a'), false, false, false),
            ),
            (
                InputEvent::Mouse(mouse_event(MouseButtons::VERT_WHEEL, Modifiers::empty())),
                input(Key::MouseScrollDown, false, false, false),
            ),
            (
                InputEvent::PixelMouse(pixel_mouse_event(
                    MouseButtons::VERT_WHEEL,
                    Modifiers::empty(),
                )),
                input(Key::MouseScrollDown, false, false, false),
            ),
            (
                InputEvent::Paste("x".into()),
                input(Key::Null, false, false, false),
            ),
        ] {
            assert_eq!(Input::from(from.clone()), to, "{:?} -> {:?}", from, to);
        }
    }
}
