use std::time::{Duration, Instant};
use crossterm::event::{Event, MouseEvent, MouseEventKind};
use bookrat::main_app::{App, run_app_with_event_source};
use ratatui::{backend::TestBackend, Terminal};
use anyhow::Result;

struct FloodEventSource {
    events: Vec<Event>,
    current_index: usize,
}

impl FloodEventSource {
    fn new_with_horizontal_scroll_flood() -> Self {
        let mut events = Vec::new();
        
        // Simulate rapid horizontal scroll flood (like the user's issue)
        for i in 0..1000 {
            events.push(Event::Mouse(MouseEvent {
                kind: if i % 2 == 0 { MouseEventKind::ScrollLeft } else { MouseEventKind::ScrollRight },
                column: 10,
                row: 10,
                modifiers: crossterm::event::KeyModifiers::empty(),
            }));
        }
        
        // Add a quit event at the end
        events.push(Event::Key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('q'),
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        }));
        
        Self {
            events,
            current_index: 0,
        }
    }
}

impl bookrat::event_source::EventSource for FloodEventSource {
    fn poll(&mut self, _timeout: Duration) -> Result<bool> {
        Ok(self.current_index < self.events.len())
    }
    
    fn read(&mut self) -> Result<Event> {
        if self.current_index < self.events.len() {
            let event = self.events[self.current_index].clone();
            self.current_index += 1;
            Ok(event)
        } else {
            Err(anyhow::anyhow!("No more events"))
        }
    }
}

#[test]
fn test_horizontal_scroll_flood_performance() {
    // Create test app
    let mut app = App::new_with_config(
        Some("tests/fixtures"),
        Some("test_bookmarks.json"),
        false,
    );
    
    // Create terminal with test backend
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    
    // Create flood event source with 1000 horizontal scroll events
    let mut event_source = FloodEventSource::new_with_horizontal_scroll_flood();
    
    // Measure time to process all events
    let start_time = Instant::now();
    let result = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);
    let elapsed = start_time.elapsed();
    
    // Assert the app didn't crash
    assert!(result.is_ok(), "App should handle flood of horizontal scroll events without crashing");
    
    // Assert it completed in reasonable time (should be < 1 second, not 5+ seconds)
    assert!(
        elapsed < Duration::from_secs(1),
        "Processing 1000 horizontal scroll events took {}ms, should be < 1000ms. This indicates event flooding issue!",
        elapsed.as_millis()
    );
    
    println!("✓ Processed 1000 horizontal scroll events in {}ms", elapsed.as_millis());
}

#[test]
fn test_mixed_scroll_events_performance() {
    // Create test app
    let mut app = App::new_with_config(
        Some("tests/fixtures"),
        Some("test_bookmarks.json"),
        false,
    );
    
    // Create terminal with test backend
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    
    // Create event source with mixed scroll events
    let mut events = Vec::new();
    
    // Add 500 horizontal scrolls interspersed with vertical scrolls
    for i in 0..500 {
        // Add horizontal scroll
        events.push(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 10,
            row: 10,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }));
        
        // Every 10th event, add a vertical scroll
        if i % 10 == 0 {
            events.push(Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 10,
                modifiers: crossterm::event::KeyModifiers::empty(),
            }));
        }
    }
    
    // Add quit event
    events.push(Event::Key(crossterm::event::KeyEvent {
        code: crossterm::event::KeyCode::Char('q'),
        modifiers: crossterm::event::KeyModifiers::empty(),
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    }));
    
    let mut event_source = FloodEventSource { events, current_index: 0 };
    
    // Measure time
    let start_time = Instant::now();
    let result = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);
    let elapsed = start_time.elapsed();
    
    // Assert performance
    assert!(result.is_ok(), "App should handle mixed scroll events");
    assert!(
        elapsed < Duration::from_millis(2000),
        "Processing mixed scroll events took {}ms, should be < 2000ms",
        elapsed.as_millis()
    );
    
    println!("✓ Processed mixed scroll events in {}ms", elapsed.as_millis());
}