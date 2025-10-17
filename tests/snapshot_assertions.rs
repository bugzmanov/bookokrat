use snapbox::{Data, assert_data_eq};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

pub fn assert_svg_snapshot(
    actual: String,
    snapshot_path: &Path,
    test_name: &str,
    on_failure: impl FnOnce(String, String, String, usize, usize, usize, Option<usize>),
) {
    // First try the snapbox assertion - this handles SNAPSHOTS=overwrite automatically
    let result = catch_unwind(AssertUnwindSafe(|| {
        assert_data_eq!(actual.clone(), Data::read_from(snapshot_path, None));
    }));

    // If the assertion succeeded, we're done
    if result.is_ok() {
        return;
    }

    // If it failed, generate our custom report
    let expected_data = Data::read_from(snapshot_path, None);
    let expected = expected_data.to_string();

    // Count differences for summary
    let actual_lines: Vec<&str> = actual.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();
    let mut diff_count = 0;
    let mut first_diff_line = None;

    for (i, (exp_line, act_line)) in expected_lines.iter().zip(actual_lines.iter()).enumerate() {
        if exp_line != act_line {
            diff_count += 1;
            if first_diff_line.is_none() {
                first_diff_line = Some(i + 1);
            }
        }
    }

    // Store line counts before moving strings
    let expected_line_count = expected_lines.len();
    let actual_line_count = actual_lines.len();

    // Call the failure callback
    on_failure(
        expected,
        actual,
        snapshot_path.to_string_lossy().to_string(),
        expected_line_count,
        actual_line_count,
        diff_count,
        first_diff_line,
    );

    // Print a concise error message
    eprintln!("\n❌ SVG snapshot test failed: {test_name}");
    eprintln!(
        "   📊 Total lines: {expected_line_count} (expected) vs {actual_line_count} (actual)"
    );
    eprintln!("   ⚠️  Lines with differences: {diff_count}");
    if let Some(line) = first_diff_line {
        eprintln!("   📍 First difference at line: {line}");
    }
    eprintln!("   💡 To update snapshot: SNAPSHOTS=overwrite cargo test {test_name}\n");

    // Panic with a clean message
    panic!("SVG snapshot mismatch");
}
