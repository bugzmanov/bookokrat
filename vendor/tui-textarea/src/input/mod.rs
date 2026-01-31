#[cfg(any(
    feature = "crossterm",
    feature = "crossterm_0_28",
    feature = "tuirs-crossterm"
))]
mod crossterm;
#[cfg(any(feature = "termion", feature = "tuirs-termion"))]
mod termion;
#[cfg(feature = "termwiz")]
mod termwiz;

#[cfg(feature = "arbitrary")]
use arbitrary::Arbitrary;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Backend-agnostic key input kind.
///
/// This type is marked as `#[non_exhaustive]` since more keys may be supported in the future.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
#[cfg_attr(feature = "arbitrary", derive(Arbitrary))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Key {
    /// Normal letter key input
    Char(char),
    /// F1, F2, F3, ... keys
    F(u8),
    /// Backspace key
    Backspace,
    /// Enter or return key
    Enter,
    /// Left arrow key
    Left,
    /// Right arrow key
    Right,
    /// Up arrow key
    Up,
    /// Down arrow key
    Down,
    /// Tab key
    Tab,
    /// Delete key
    Delete,
    /// Home key
    Home,
    /// End key
    End,
    /// Page up key
    PageUp,
    /// Page down key
    PageDown,
    /// Escape key
    Esc,
    /// Copy key. This key is supported by termwiz only
    Copy,
    /// Cut key. This key is supported by termwiz only
    Cut,
    /// Paste key. This key is supported by termwiz only
    Paste,
    /// Virtual key to scroll down by mouse
    MouseScrollDown,
    /// Virtual key to scroll up by mouse
    MouseScrollUp,
    /// An invalid key input (this key is always ignored by [`TextArea`](crate::TextArea))
    Null,
}

impl Default for Key {
    fn default() -> Self {
        Key::Null
    }
}

/// Backend-agnostic key input type.
///
/// When `crossterm`, `termion`, `termwiz` features are enabled, converting respective key input types into this
/// `Input` type is defined.
/// ```no_run
/// use tui_textarea::{TextArea, Input, Key};
/// use crossterm::event::{Event, read};
///
/// let event = read().unwrap();
///
/// // `Input::from` can convert backend-native event into `Input`
/// let input = Input::from(event.clone());
/// // or `Into::into`
/// let input: Input = event.clone().into();
/// // Conversion from `KeyEvent` value is also available
/// if let Event::Key(key) = event {
///     let input = Input::from(key);
/// }
/// ```
///
/// Creating `Input` instance directly can cause backend-agnostic input as follows.
///
/// ```
/// use tui_textarea::{TextArea, Input, Key};
///
/// let mut textarea = TextArea::default();
///
/// // Input Ctrl+A
/// textarea.input(Input {
///     key: Key::Char('a'),
///     ctrl: true,
///     alt: false,
///     shift: false,
/// });
/// ```
#[derive(Debug, Clone, Default, PartialEq, Hash, Eq)]
#[cfg_attr(feature = "arbitrary", derive(Arbitrary))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Input {
    /// Typed key.
    pub key: Key,
    /// Ctrl modifier key. `true` means Ctrl key was pressed.
    pub ctrl: bool,
    /// Alt modifier key. `true` means Alt key was pressed.
    pub alt: bool,
    /// Shift modifier key. `true` means Shift key was pressed.
    pub shift: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    pub(crate) fn input(key: Key, ctrl: bool, alt: bool, shift: bool) -> Input {
        Input {
            key,
            ctrl,
            alt,
            shift,
        }
    }

    #[test]
    #[cfg(feature = "arbitrary")]
    fn arbitrary_input() {
        let mut u = arbitrary::Unstructured::new(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        Input::arbitrary(&mut u).unwrap();
    }

    #[test]
    #[cfg(feature = "arbitrary")]
    fn arbitrary_key() {
        let mut u = arbitrary::Unstructured::new(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        Key::arbitrary(&mut u).unwrap();
    }
}
