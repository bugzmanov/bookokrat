# BookRat Snapshot Testing Infrastructure

This document explains how to write snapshot tests for the BookRat TUI application.

## Overview

The testing infrastructure allows you to simulate user keyboard input and capture the resulting terminal output for snapshot testing. This enables testing of the full UI behavior without requiring manual interaction.

## Key Components

### 1. Event Source Abstraction (`src/event_source.rs`)

The `EventSource` trait abstracts keyboard input, allowing both real keyboard events and simulated events:

```rust
pub trait EventSource {
    fn poll(&mut self, timeout: Duration) -> Result<bool>;
    fn read(&mut self) -> Result<Event>;
}
```

- `KeyboardEventSource`: Real keyboard input (used in production)
- `SimulatedEventSource`: Simulated input for testing

### 2. Test Scenario Builder (`src/test_utils.rs`)

The `TestScenarioBuilder` provides a fluent API for creating test scenarios:

```rust
let scenario = TestScenarioBuilder::new()
    .navigate_down(2)      // Press 'j' twice
    .press_enter()         // Select file
    .press_tab()           // Switch to content view
    .scroll_down(5)        // Scroll down 5 times
    .half_screen_down()    // Ctrl+d
    .next_chapter()        // Press 'l'
    .quit()                // Press 'q'
    .build();
```

### 3. Terminal Capture

The test utilities provide functions to:
- Create a test terminal with specific dimensions
- Capture the terminal buffer as a string for snapshot comparison

## Writing Tests

### Example Test Structure

```rust
#[test]
fn test_file_navigation_and_reading() {
    // 1. Create test terminal
    let mut terminal = create_test_terminal(80, 24);
    
    // 2. Create app instance
    let mut app = App::new();
    
    // 3. Create test scenario
    let mut event_source = TestScenarioBuilder::new()
        .navigate_down(1)     // Select second file
        .press_enter()        // Open file
        .navigate_down(10)    // Scroll in content
        .press_tab()          // Switch back to file list
        .quit()
        .build();
    
    // 4. Run the app with simulated events
    // Note: You would need to set up test EPUB files first
    run_app_with_event_source(&mut terminal, &mut app, &mut event_source)?;
    
    // 5. Capture snapshots at key moments
    terminal.draw(|f| app.draw(f))?;
    let snapshot = capture_terminal_state(&terminal);
    
    // 6. Compare with expected snapshot
    // Using a snapshot testing library like insta:
    insta::assert_snapshot!(snapshot);
}
```

### Available Test Actions

The `TestScenarioBuilder` supports all keyboard shortcuts:

- **Navigation**: `navigate_down(n)`, `navigate_up(n)`
- **File Operations**: `press_enter()` (select file)
- **View Switching**: `press_tab()`
- **Content Scrolling**: 
  - Line scrolling: via `navigate_down/up`
  - Half-screen: `half_screen_down()`, `half_screen_up()`
- **Chapter Navigation**: `next_chapter()`, `prev_chapter()`
- **Exit**: `quit()`

### Custom Key Combinations

For keys not covered by the builder:

```rust
// Direct character key
SimulatedEventSource::char_key('x')

// Ctrl+key combination
SimulatedEventSource::ctrl_char_key('x')

// Other key codes
SimulatedEventSource::key_event(KeyCode::F(1), KeyModifiers::empty())
```

## Test Data Setup

For complete integration tests, you'll need:

1. Test EPUB files in a known location
2. Mock or test bookmark data
3. Controlled environment (working directory, etc.)

## Running Tests

```bash
# Run all tests
cargo test

# Run snapshot tests specifically
cargo test snapshot_tests

# Update snapshots (if using insta)
cargo insta review
```

## Best Practices

1. **Deterministic Tests**: Ensure test scenarios produce consistent output
2. **Focused Tests**: Test one behavior per test
3. **Clear Naming**: Use descriptive test names that explain the scenario
4. **Snapshot Organization**: Keep snapshots organized by test module
5. **Terminal Size**: Use consistent terminal dimensions for comparable snapshots

## Future Enhancements

- Add time-based event simulation (for animations)
- Support for mouse events
- Parallel test execution with isolated environments
- Automated EPUB test data generation