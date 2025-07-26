use bookrat::main_app::run_app_with_event_source;
use bookrat::main_app::{App, MockSystemCommandExecutor, Mode};
use bookrat::test_utils::test_helpers::TestScenarioBuilder;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn test_ctrl_o_opens_system_viewer_when_epub_loaded() {
    // Create a mock system command executor
    let mock_executor = MockSystemCommandExecutor::new();

    // Create app with mock executor
    let mut app = App::new_with_mock_system_executor(
        Some("tests/testdata"),
        Some("/dev/null"),
        false,
        mock_executor,
    );

    // Load an EPUB file first
    app.load_epub("tests/testdata/digital_frontier.epub");
    app.mode = Mode::Content; // Ensure we're in content mode

    // Create event source with Ctrl+O followed by quit
    let mut event_source = TestScenarioBuilder::new().press_ctrl_o().quit().build();

    // Create a test terminal
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // Run the app with the simulated input
    let _ = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Verify that the system command was executed
    let executed_commands = app
        .system_command_executor
        .as_any()
        .downcast_ref::<MockSystemCommandExecutor>()
        .unwrap()
        .get_executed_commands();

    assert_eq!(executed_commands.len(), 1);
    assert_eq!(executed_commands[0], "tests/testdata/digital_frontier.epub@chapter1");
}

#[test]
fn test_ctrl_o_does_nothing_in_file_list_mode() {
    // Create a mock system command executor
    let mock_executor = MockSystemCommandExecutor::new();

    // Create app with mock executor
    let mut app = App::new_with_mock_system_executor(
        Some("tests/testdata"),
        Some("/dev/null"),
        false,
        mock_executor,
    );

    // Stay in file list mode (don't load any EPUB)
    assert_eq!(app.mode, Mode::FileList);

    // Create event source with Ctrl+O followed by quit
    let mut event_source = TestScenarioBuilder::new().press_ctrl_o().quit().build();

    // Create a test terminal
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // Run the app with the simulated input
    let _ = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Verify that no system command was executed
    let executed_commands = app
        .system_command_executor
        .as_any()
        .downcast_ref::<MockSystemCommandExecutor>()
        .unwrap()
        .get_executed_commands();

    assert_eq!(executed_commands.len(), 0);
}

#[test]
fn test_ctrl_o_does_nothing_when_no_epub_loaded() {
    // Create a mock system command executor
    let mock_executor = MockSystemCommandExecutor::new();

    // Create app with mock executor
    let mut app = App::new_with_mock_system_executor(
        Some("tests/testdata"),
        Some("/dev/null"),
        false,
        mock_executor,
    );

    // Switch to content mode but don't load any EPUB
    app.mode = Mode::Content;

    // Create event source with Ctrl+O followed by quit
    let mut event_source = TestScenarioBuilder::new().press_ctrl_o().quit().build();

    // Create a test terminal
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // Run the app with the simulated input
    let _ = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Verify that no system command was executed
    let executed_commands = app
        .system_command_executor
        .as_any()
        .downcast_ref::<MockSystemCommandExecutor>()
        .unwrap()
        .get_executed_commands();

    assert_eq!(executed_commands.len(), 0);
}
