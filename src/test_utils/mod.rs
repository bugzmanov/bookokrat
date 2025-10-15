pub mod simple_fake_books;

pub mod test_helpers {
    use super::simple_fake_books::create_test_books_in_dir;
    use crate::event_source::{Event, KeyCode, KeyEvent, KeyModifiers, SimulatedEventSource};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Builder for creating test scenarios with simulated user input
    pub struct TestScenarioBuilder {
        events: Vec<Event>,
    }

    impl Default for TestScenarioBuilder {
        fn default() -> Self {
            Self::new()
        }
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

        /// Press Ctrl+O (Open with system viewer)
        pub fn press_ctrl_o(mut self) -> Self {
            self.events.push(Event::Key(KeyEvent {
                code: KeyCode::Char('o'),
                modifiers: KeyModifiers::CONTROL,
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
        let mut terminal = Terminal::new(backend).unwrap();
        // Hide cursor for test terminals to prevent it from appearing in SVG snapshots
        terminal.hide_cursor().unwrap();
        terminal
    }

    /// Capture the current terminal buffer as a string
    pub fn capture_terminal_state(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut lines = Vec::new();

        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
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

    /// Create a test App instance with clean initial conditions
    /// - Uses testdata directory for EPUBs
    /// - No bookmark file (starts with empty bookmarks)
    /// - No auto-loading of recent books
    pub fn create_test_app() -> crate::App {
        crate::App::new_with_config(
            Some("tests/testdata"), // Use tests/testdata directory
            Some("/dev/null"),      // Non-existent bookmark file = empty bookmarks
            false,                  // Don't auto-load recent books
        )
    }

    /// Creates temporary fake EPUB files for testing
    pub struct TempBookManager {
        temp_dir: tempfile::TempDir,
        book_paths: Vec<String>,
    }

    impl TempBookManager {
        pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let temp_dir = tempfile::TempDir::new()?;
            let book_paths = create_test_books_in_dir(temp_dir.path())?;

            Ok(Self {
                temp_dir,
                book_paths,
            })
        }

        pub fn new_with_configs(
            configs: &[crate::simple_fake_books::FakeBookConfig],
        ) -> Result<Self, Box<dyn std::error::Error>> {
            let temp_dir = tempfile::TempDir::new()?;
            let book_paths = crate::simple_fake_books::create_custom_test_books_in_dir(
                temp_dir.path(),
                configs,
            )?;

            Ok(Self {
                temp_dir,
                book_paths,
            })
        }

        /// Get the temporary directory path
        pub fn get_directory(&self) -> String {
            self.temp_dir.path().to_string_lossy().to_string()
        }

        /// Get book paths that were created
        pub fn get_book_paths(&self) -> &[String] {
            &self.book_paths
        }
    }

    impl Default for TempBookManager {
        fn default() -> Self {
            Self::new().expect("Failed to create temporary directory")
        }
    }

    /// Create a test App instance with custom fake books
    /// - Uses temporary directory with specified fake EPUB files
    /// - No bookmark file (starts with empty bookmarks)
    /// - No auto-loading of recent books
    pub fn create_test_app_with_custom_fake_books(
        configs: &[crate::simple_fake_books::FakeBookConfig],
    ) -> (crate::App, TempBookManager) {
        let temp_manager =
            TempBookManager::new_with_configs(configs).expect("Failed to create temp books");

        let app = crate::App::new_with_config(
            Some(&temp_manager.get_directory()), // Use temporary directory with fake books
            Some("/dev/null"),                   // Non-existent bookmark file = empty bookmarks
            false,                               // Don't auto-load recent books
        );

        (app, temp_manager)
    }

    /// Create a test App instance with standard fake books for consistent testing (backward compatibility)
    /// - Uses temporary directory with fake EPUB files
    /// - No bookmark file (starts with empty bookmarks)
    /// - No auto-loading of recent books
    pub fn create_test_app_with_fake_books() -> (crate::App, TempBookManager) {
        let temp_manager = TempBookManager::new().expect("Failed to create temp books");

        let app = crate::App::new_with_config(
            Some(&temp_manager.get_directory()), // Use temporary directory with fake books
            Some("/dev/null"),                   // Non-existent bookmark file = empty bookmarks
            false,                               // Don't auto-load recent books
        );

        (app, temp_manager)
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
