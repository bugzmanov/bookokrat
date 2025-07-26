use bookrat::panic_handler;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
/// Example to test panic handler mouse restoration
///
/// This example can be used to verify that mouse functionality is properly
/// restored after a panic. Run with: cargo run --example panic_test
///
/// Instructions:
/// 1. Run the example
/// 2. Try using mouse scroll/selection (should work)
/// 3. Press 'p' to trigger a panic
/// 4. After the panic, try mouse scroll/selection in the terminal (should still work)
use std::io::stdout;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize panic handler
    panic_handler::initialize_panic_handler();

    println!("Panic Test Example");
    println!("This will test mouse capture restoration after a panic.");
    println!("Instructions:");
    println!("1. Press any key to enter TUI mode");
    println!("2. Try mouse scroll/selection (should work)");
    println!("3. Press 'p' to trigger a panic");
    println!("4. After panic, test mouse in terminal again");
    println!("\nPress any key to continue...");

    // Wait for input
    let _ = std::io::stdin().read_line(&mut String::new());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    println!("TUI Mode Active!");
    println!("Mouse capture is enabled - try scrolling!");
    println!("Press 'p' to panic, 'q' to quit normally");

    loop {
        match event::read()? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('p') => {
                    println!("Triggering panic to test restoration...");
                    panic!("Test panic - mouse should still work after this!");
                }
                _ => {}
            },
            Event::Mouse(mouse) => {
                println!("Mouse event: {:?}", mouse);
            }
            _ => {}
        }
    }

    // Normal cleanup
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    println!("Exited normally - mouse should work in terminal");

    Ok(())
}
