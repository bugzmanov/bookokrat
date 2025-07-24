pub mod test_helpers {
    use crate::event_source::{Event, KeyCode, KeyEvent, KeyModifiers, SimulatedEventSource};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Builder for creating test scenarios with simulated user input
    pub struct TestScenarioBuilder {
        events: Vec<Event>,
    }

    impl TestScenarioBuilder {
        pub fn new() -> Self {
            Self { events: Vec::new() }
        }

        /// Add a character key press
        pub fn press_char(mut self, c: char) -> Self {
            self.events.push(SimulatedEventSource::char_key(c));
            self
        }

        /// Add a Ctrl+character key press
        pub fn press_ctrl_char(mut self, c: char) -> Self {
            self.events.push(SimulatedEventSource::ctrl_char_key(c));
            self
        }

        /// Press Enter
        pub fn press_enter(mut self) -> Self {
            self.events.push(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::empty(),
                kind: crossterm::event::KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            }));
            self
        }

        /// Press Tab
        pub fn press_tab(mut self) -> Self {
            self.events.push(Event::Key(KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::empty(),
                kind: crossterm::event::KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            }));
            self
        }

        /// Navigate down n times (press 'j' n times)
        pub fn navigate_down(mut self, times: usize) -> Self {
            for _ in 0..times {
                self.events.push(SimulatedEventSource::char_key('j'));
            }
            self
        }

        /// Navigate up n times (press 'k' n times)
        pub fn navigate_up(mut self, times: usize) -> Self {
            for _ in 0..times {
                self.events.push(SimulatedEventSource::char_key('k'));
            }
            self
        }

        /// Navigate to next chapter (press 'l')
        pub fn next_chapter(mut self) -> Self {
            self.events.push(SimulatedEventSource::char_key('l'));
            self
        }

        /// Navigate to previous chapter (press 'h')
        pub fn prev_chapter(mut self) -> Self {
            self.events.push(SimulatedEventSource::char_key('h'));
            self
        }

        /// Scroll half screen down (Ctrl+d)
        pub fn half_screen_down(mut self) -> Self {
            self.events.push(SimulatedEventSource::ctrl_char_key('d'));
            self
        }

        /// Scroll half screen up (Ctrl+u)
        pub fn half_screen_up(mut self) -> Self {
            self.events.push(SimulatedEventSource::ctrl_char_key('u'));
            self
        }

        /// Quit the application (press 'q')
        pub fn quit(mut self) -> Self {
            self.events.push(SimulatedEventSource::char_key('q'));
            self
        }

        /// Build the simulated event source
        pub fn build(self) -> SimulatedEventSource {
            SimulatedEventSource::new(self.events)
        }
    }

    /// Create a test terminal for snapshot testing
    pub fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        Terminal::new(backend).unwrap()
    }

    /// Capture the current terminal buffer as a string
    pub fn capture_terminal_state(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut lines = Vec::new();

        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = buffer.get(x, y);
                line.push_str(cell.symbol());
            }
            // Trim trailing whitespace from each line
            lines.push(line.trim_end().to_string());
        }

        // Remove trailing empty lines
        while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }

        lines.join("\n")
    }

}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;

    #[test]
    fn test_scenario_builder() {
        let scenario = TestScenarioBuilder::new()
            .navigate_down(2)
            .press_enter()
            .press_tab()
            .navigate_up(1)
            .quit()
            .build();

        // Verify the events were created correctly
        let events = scenario.events;
        assert_eq!(events.len(), 6);
    }
}
