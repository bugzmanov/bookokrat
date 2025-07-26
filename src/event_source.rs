use anyhow::Result;
pub use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
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

        // Should have events available
        assert!(source.poll(Duration::from_millis(0)).unwrap());

        // Read first event
        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('j'));
            assert!(key.modifiers.is_empty());
        }

        // Read second event
        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('k'));
            assert!(key.modifiers.is_empty());
        }

        // Read third event
        if let Event::Key(key) = source.read().unwrap() {
            assert_eq!(key.code, KeyCode::Char('d'));
            assert!(key.modifiers.contains(KeyModifiers::CONTROL));
        }

        // No more events
        assert!(!source.poll(Duration::from_millis(0)).unwrap());
    }
}
