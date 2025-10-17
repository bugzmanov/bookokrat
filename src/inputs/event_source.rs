use anyhow::Result;
pub use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Duration;

/// Trait for abstracting event sources to enable testing
pub trait EventSource {
    /// Poll for events with a timeout
    fn poll(&mut self, timeout: Duration) -> Result<bool>;

    /// Read the next event
    fn read(&mut self) -> Result<Event>;
}

/// Real keyboard event source using crossterm
pub struct KeyboardEventSource;

impl EventSource for KeyboardEventSource {
    fn poll(&mut self, timeout: Duration) -> Result<bool> {
        Ok(crossterm::event::poll(timeout)?)
    }

    fn read(&mut self) -> Result<Event> {
        Ok(crossterm::event::read()?)
    }
}

/// Simulated event source for testing
pub struct SimulatedEventSource {
    pub(crate) events: Vec<Event>,
    current_index: usize,
}

impl SimulatedEventSource {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            current_index: 0,
        }
    }

    /// Helper method to create a key event
    pub fn key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        })
    }

    /// Helper method to create a simple character key event
    pub fn char_key(c: char) -> Event {
        Self::key_event(KeyCode::Char(c), KeyModifiers::empty())
    }

    /// Helper method to create a Ctrl+char key event
    pub fn ctrl_char_key(c: char) -> Event {
        Self::key_event(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    /// Helper method to create a mouse scroll down event
    pub fn mouse_scroll_down(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }

    /// Helper method to create a mouse scroll up event  
    pub fn mouse_scroll_up(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }

    /// Helper method to create a mouse button down event
    pub fn mouse_down(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }

    /// Helper method to create a mouse button up event
    pub fn mouse_up(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }

    /// Helper method to create a mouse drag event
    pub fn mouse_drag(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }
}

impl EventSource for SimulatedEventSource {
    fn poll(&mut self, _timeout: Duration) -> Result<bool> {
        Ok(self.current_index < self.events.len())
    }

    fn read(&mut self) -> Result<Event> {
        if self.current_index < self.events.len() {
            let event = self.events[self.current_index].clone();
            self.current_index += 1;
            Ok(event)
        } else {
            // Return a quit event if we've exhausted all events
            Ok(SimulatedEventSource::char_key('q'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulated_event_source() {
        let events = vec![
            SimulatedEventSource::char_key('j'),
            SimulatedEventSource::char_key('k'),
            SimulatedEventSource::ctrl_char_key('d'),
        ];

        let mut source = SimulatedEventSource::new(events);

        assert!(source.poll(Duration::from_millis(0)).unwrap());

        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('j'));
            assert!(key.modifiers.is_empty());
        }

        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('k'));
            assert!(key.modifiers.is_empty());
        }

        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('d'));
            assert!(key.modifiers.contains(KeyModifiers::CONTROL));
        }

        assert!(!source.poll(Duration::from_millis(0)).unwrap());
    }

    #[test]
    fn test_mouse_events() {
        let events = vec![
            SimulatedEventSource::mouse_scroll_down(50, 15),
            SimulatedEventSource::mouse_scroll_up(25, 10),
        ];

        let mut source = SimulatedEventSource::new(events);

        assert!(source.poll(Duration::from_millis(0)).unwrap());

        if let Event::Mouse(mouse) = source.read().unwrap() {
            assert_eq!(mouse.kind, MouseEventKind::ScrollDown);
            assert_eq!(mouse.column, 50);
            assert_eq!(mouse.row, 15);
            assert!(mouse.modifiers.is_empty());
        } else {
            panic!("Expected mouse event");
        }

        if let Event::Mouse(mouse) = source.read().unwrap() {
            assert_eq!(mouse.kind, MouseEventKind::ScrollUp);
            assert_eq!(mouse.column, 25);
            assert_eq!(mouse.row, 10);
            assert!(mouse.modifiers.is_empty());
        } else {
            panic!("Expected mouse event");
        }

        assert!(!source.poll(Duration::from_millis(0)).unwrap());
    }
}
