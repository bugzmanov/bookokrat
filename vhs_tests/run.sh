#!/usr/bin/env bash
#
# VHS Terminal Screenshot Test Harness
#
# A tape-recorder style test runner for visual regression testing
# of terminal applications in real terminals.
#
# Usage:
#   ./vhs_tests/run.sh                           # Run all tapes (ghostty)
#   ./vhs_tests/run.sh --terminal kitty          # Run with Kitty (background-friendly!)
#   ./vhs_tests/run.sh --tape pdf_smoke          # Run specific tape
#   ./vhs_tests/run.sh --tape pdf_smoke --update # Update golden snapshots
#   ./vhs_tests/run.sh --list                    # List available tapes
#   ./vhs_tests/run.sh --open-report             # Open report after run
#
# Terminals:
#   ghostty - Default. Requires focus (may conflict with other Ghostty windows)
#   kitty   - Background-friendly! Requires: kitty -o allow_remote_control=yes
#
# Requirements:
#   - macOS
#   - Ghostty.app or Kitty installed
#   - For Ghostty: Accessibility permissions for keystroke automation
#   - For Kitty: Remote control enabled (allow_remote_control yes)

set -e

# Cleanup on exit (trap ensures cleanup even if script fails)
cleanup_on_exit() {
    # Cleanup functions may not be defined yet if script fails early
    case "$TERMINAL_TYPE" in
        kitty)
            if type cleanup_kitty &>/dev/null; then
                cleanup_kitty 2>/dev/null || true
            fi
            ;;
        ghostty)
            if type cleanup_ghostty &>/dev/null; then
                cleanup_ghostty 2>/dev/null || true
            fi
            ;;
        wezterm)
            if type cleanup_wezterm &>/dev/null; then
                cleanup_wezterm 2>/dev/null || true
            fi
            ;;
        iterm)
            if type cleanup_iterm &>/dev/null; then
                cleanup_iterm 2>/dev/null || true
            fi
            ;;
    esac
}
trap cleanup_on_exit EXIT

# Resolve script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default terminal type (can be overridden with --terminal)
TERMINAL_TYPE=${TERMINAL_TYPE:-ghostty}

# Source library functions (terminal-specific manager sourced after parsing args)
source "$SCRIPT_DIR/lib/image_compare.sh"
source "$SCRIPT_DIR/lib/report_generator.sh"

# Configuration
TAPES_DIR="$SCRIPT_DIR/tapes"
GOLDEN_DIR="$SCRIPT_DIR/golden"
OUTPUT_DIR="$SCRIPT_DIR/output"
SCREENSHOTS_DIR="$OUTPUT_DIR/screenshots"
REPORTS_DIR="$OUTPUT_DIR/reports"

BINARY="$PROJECT_ROOT/target/release/bookokrat"
TEST_PDF="$PROJECT_ROOT/tests/testdata/vhs_test.pdf"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Arguments
UPDATE_MODE=false
SPECIFIC_TAPE=""
LIST_TAPES=false
OPEN_REPORT=false
VERBOSE=false
MEMORY_LEAK_LIMIT_MB=100  # Default: fail if memory leak > 100MB (includes ~80MB harness overhead)

print_usage() {
    echo "VHS Terminal Screenshot Test Harness"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --terminal TYPE          Terminal to use: ghostty (default) or kitty"
    echo "  --tape NAME              Run specific tape (without .tape extension)"
    echo "  --update                 Update golden snapshots instead of comparing"
    echo "  --list                   List available tapes"
    echo "  --open-report            Open HTML report after run"
    echo "  --verbose                Enable verbose output"
    echo "  --memory-leak-limit MB   Fail if memory leak exceeds MB (default: 100)"
    echo "  --help                   Show this help"
    echo ""
    echo "Terminals:"
    echo "  ghostty   Default. May require window focus (conflicts with other Ghostty windows)"
    echo "  kitty     Background-friendly! Requires: allow_remote_control yes in kitty.conf"
    echo ""
    echo "Examples:"
    echo "  $0                                    # Run all tapes with Ghostty"
    echo "  $0 --terminal kitty                   # Run with Kitty (background-friendly)"
    echo "  $0 --tape pdf_smoke                   # Run pdf_smoke.tape"
    echo "  $0 --terminal kitty --tape pdf_smoke --update  # Update golden with Kitty"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --terminal)
            TERMINAL_TYPE="$2"
            if [[ "$TERMINAL_TYPE" != "ghostty" && "$TERMINAL_TYPE" != "kitty" && "$TERMINAL_TYPE" != "wezterm" && "$TERMINAL_TYPE" != "iterm" ]]; then
                echo "ERROR: Unknown terminal: $TERMINAL_TYPE (use ghostty, kitty, wezterm, or iterm)"
                exit 1
            fi
            shift 2
            ;;
        --tape)
            SPECIFIC_TAPE="$2"
            shift 2
            ;;
        --update)
            UPDATE_MODE=true
            shift
            ;;
        --list)
            LIST_TAPES=true
            shift
            ;;
        --open-report)
            OPEN_REPORT=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            export VERBOSE
            shift
            ;;
        --memory-leak-limit)
            MEMORY_LEAK_LIMIT_MB="$2"
            shift 2
            ;;
        --help|-h)
            print_usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac
done

# Export terminal type for tape_runner
export TERMINAL_TYPE

# Source terminal-specific manager and tape runner
case "$TERMINAL_TYPE" in
    kitty)
        source "$SCRIPT_DIR/lib/kitty_manager.sh"
        ;;
    wezterm)
        source "$SCRIPT_DIR/lib/wezterm_manager.sh"
        ;;
    iterm)
        source "$SCRIPT_DIR/lib/iterm_manager.sh"
        ;;
    ghostty|*)
        source "$SCRIPT_DIR/lib/window_manager.sh"
        ;;
esac
source "$SCRIPT_DIR/lib/tape_runner.sh"

# Setup cleanup trap for managed terminals
cleanup_on_exit() {
    case "$TERMINAL_TYPE" in
        kitty)
            cleanup_kitty 2>/dev/null || true
            ;;
        ghostty)
            cleanup_ghostty 2>/dev/null || true
            ;;
        wezterm)
            cleanup_wezterm 2>/dev/null || true
            ;;
        iterm)
            cleanup_iterm 2>/dev/null || true
            ;;
    esac
}
trap cleanup_on_exit EXIT

# Memory measurement function (returns anonymous pages count)
get_anonymous_pages() {
    vm_stat | awk '/Anonymous pages:/ { gsub(/\./, "", $3); print $3 }'
}

# List tapes
if $LIST_TAPES; then
    echo "Available tapes:"
    for tape in "$TAPES_DIR"/*.tape; do
        if [ -f "$tape" ]; then
            name=$(basename "$tape" .tape)
            # Count commands
            cmd_count=$(grep -v '^#' "$tape" | grep -v '^$' | wc -l | tr -d ' ')
            screenshot_count=$(grep -c '^screenshot' "$tape" || echo "0")
            echo "  $name ($screenshot_count screenshots, $cmd_count commands)"
        fi
    done
    exit 0
fi

# Banner
echo ""
echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     🎬 VHS Terminal Screenshot Test Harness                ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check prerequisites
echo "Checking prerequisites (terminal: $TERMINAL_TYPE)..."
case "$TERMINAL_TYPE" in
    kitty)
        if ! check_kitty; then
            exit 1
        fi
        ;;
    wezterm)
        if ! check_wezterm; then
            exit 1
        fi
        ;;
    iterm)
        if ! check_iterm; then
            exit 1
        fi
        ;;
    ghostty|*)
        if ! check_ghostty; then
            exit 1
        fi
        ;;
esac

if [ ! -f "$TEST_PDF" ]; then
    echo -e "${RED}ERROR: Test PDF not found: $TEST_PDF${NC}"
    exit 1
fi

# Build if needed
if [ ! -f "$BINARY" ]; then
    echo "Building release binary with PDF support..."
    (cd "$PROJECT_ROOT" && cargo build --release --features pdf)
fi

# Create output directories
mkdir -p "$SCREENSHOTS_DIR"
mkdir -p "$REPORTS_DIR"
mkdir -p "$GOLDEN_DIR"

# Collect tapes to run
tapes_to_run=()
if [ -n "$SPECIFIC_TAPE" ]; then
    tape_file="$TAPES_DIR/$SPECIFIC_TAPE.tape"
    if [ ! -f "$tape_file" ]; then
        echo -e "${RED}ERROR: Tape not found: $tape_file${NC}"
        echo "Use --list to see available tapes"
        exit 1
    fi
    tapes_to_run+=("$tape_file")
else
    for tape in "$TAPES_DIR"/*.tape; do
        if [ -f "$tape" ]; then
            tapes_to_run+=("$tape")
        fi
    done
fi

if [ ${#tapes_to_run[@]} -eq 0 ]; then
    echo -e "${YELLOW}No tapes found in $TAPES_DIR${NC}"
    echo "Create a .tape file to get started"
    exit 0
fi

echo "Found ${#tapes_to_run[@]} tape(s) to run"
echo "Memory leak limit: ${MEMORY_LEAK_LIMIT_MB} MB"

# Measure memory before tests
MEMORY_BEFORE=$(get_anonymous_pages)

# Run each tape
total_passed=0
total_failed=0
reports_generated=()

for tape_file in "${tapes_to_run[@]}"; do
    tape_name=$(basename "$tape_file" .tape)
    tape_screenshots_dir="$SCREENSHOTS_DIR/$TERMINAL_TYPE/$tape_name"
    tape_golden_dir="$GOLDEN_DIR/$TERMINAL_TYPE/$tape_name"
    tape_report="$REPORTS_DIR/${TERMINAL_TYPE}_${tape_name}_report.html"

    mkdir -p "$tape_screenshots_dir"
    mkdir -p "$tape_golden_dir"

    # Run the tape (|| true prevents set -e from exiting on tape errors)
    run_tape "$tape_file" "$BINARY" "$TEST_PDF" "$tape_screenshots_dir" || true

    # Get screenshots that were taken (parse from tape file)
    screenshots=($(grep '^screenshot' "$tape_file" | awk '{print $2}'))

    if [ ${#screenshots[@]} -eq 0 ]; then
        echo -e "${YELLOW}No screenshots in tape: $tape_name${NC}"
        continue
    fi

    if $UPDATE_MODE; then
        echo ""
        echo -e "${YELLOW}Updating golden snapshots...${NC}"
        update_golden_snapshots "$tape_screenshots_dir" "$tape_golden_dir" "${screenshots[@]}"
        echo -e "${GREEN}✓ Golden snapshots updated for $tape_name${NC}"
    else
        # Generate report and compare
        echo ""
        echo "Generating report..."
        if generate_report "$tape_name" "$tape_golden_dir" "$tape_screenshots_dir" "$tape_report" "${screenshots[@]}"; then
            total_passed=$((total_passed + 1))
        else
            total_failed=$((total_failed + 1))
        fi
        reports_generated+=("$tape_report")
    fi
done

# Measure memory after tests
MEMORY_AFTER=$(get_anonymous_pages)
MEMORY_DELTA_PAGES=$((MEMORY_AFTER - MEMORY_BEFORE))
MEMORY_DELTA_MB=$((MEMORY_DELTA_PAGES * 4096 / 1024 / 1024))

# Check for memory leak
MEMORY_LEAK_DETECTED=false
if [ $MEMORY_DELTA_MB -gt $MEMORY_LEAK_LIMIT_MB ]; then
    MEMORY_LEAK_DETECTED=true
fi

# Final summary
echo ""
echo "════════════════════════════════════════════════════════════"
if $UPDATE_MODE; then
    echo -e "${GREEN}Golden snapshots updated${NC}"
else
    if [ $total_failed -eq 0 ]; then
        echo -e "${GREEN}All tapes passed ✓${NC}"
    else
        echo -e "${RED}$total_failed tape(s) with failures${NC}"
    fi

    # Memory report
    echo ""
    if $MEMORY_LEAK_DETECTED; then
        echo -e "${RED}Memory leak detected: +${MEMORY_DELTA_MB} MB (limit: ${MEMORY_LEAK_LIMIT_MB} MB)${NC}"
    else
        echo -e "${GREEN}Memory check passed: +${MEMORY_DELTA_MB} MB (limit: ${MEMORY_LEAK_LIMIT_MB} MB)${NC}"
    fi

    # Open reports if requested
    if $OPEN_REPORT && [ ${#reports_generated[@]} -gt 0 ]; then
        echo ""
        echo "Opening reports..."
        for report in "${reports_generated[@]}"; do
            open "$report"
        done
    elif [ ${#reports_generated[@]} -gt 0 ]; then
        echo ""
        echo "Reports:"
        for report in "${reports_generated[@]}"; do
            echo "  $report"
        done
    fi
fi
echo "════════════════════════════════════════════════════════════"

# Cleanup managed terminal instance
case "$TERMINAL_TYPE" in
    kitty)
        cleanup_kitty
        ;;
    ghostty)
        cleanup_ghostty
        ;;
    wezterm)
        cleanup_wezterm
        ;;
    iterm)
        cleanup_iterm
        ;;
esac

# Exit with failure if any tests failed or memory leak detected
if [ $total_failed -gt 0 ]; then
    exit 1
fi

if $MEMORY_LEAK_DETECTED; then
    echo -e "${RED}FAILED: Memory leak exceeded limit${NC}"
    exit 1
fi
