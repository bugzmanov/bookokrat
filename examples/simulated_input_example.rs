/// Example demonstrating how to run the app with simulated keyboard input
/// Run with: cargo run --example simulated_input_example
use bookrat::test_utils::test_helpers::*;
use bookrat::{App, run_app_with_event_source};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn main() -> anyhow::Result<()> {
    // Create a test terminal
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;

    // Create the app
    let mut app = App::new();

    // Create a sequence of simulated user inputs
    let mut event_source = TestScenarioBuilder::new()
        .navigate_down(2) // Navigate down in file list
        .press_tab() // Switch to content view
        .navigate_down(5) // Scroll down in content
        .half_screen_down() // Ctrl+d
        .next_chapter() // Go to next chapter
        .prev_chapter() // Go back to previous chapter
        .press_tab() // Switch back to file list
        .quit() // Exit the app
        .build();

    // Run the app with simulated input
    let result = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Capture the final terminal state
    let final_state = capture_terminal_state(&terminal);

    println!("=== Final Terminal State ===");
    println!("{}", final_state);
    println!("=== End Terminal State ===");

    if let Err(e) = result {
        eprintln!("Error running app: {}", e);
    }

    Ok(())
}
