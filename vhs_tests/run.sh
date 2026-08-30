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
ACCEPT_MODE=false          # accept already-captured screenshots as golden (no re-run)
SPECIFIC_TAPE=""
SPECIFIC_SCREENSHOT=""     # limit update/accept/compare to a single screenshot
LIST_TAPES=false
OPEN_REPORT=false
VERBOSE=false
MEMORY_LEAK_LIMIT_MB=200  # Default: fail if memory leak > 200MB (includes ~80MB harness overhead; measures SYSTEM-WIDE anonymous pages via vm_stat, so other processes on the machine add noise)
EXCLUDED_DEFAULT_TAPES=(
    "demo_combined"
    "demo_epub"
    "demo_pdf"
    "docsite_epub"
    "docsite_pdf"
)

# List screenshot names a tape produces for the current terminal: plain
# `screenshot X` lines plus `@<terminal> screenshot X` conditional lines.
tape_screenshot_names() {
    local tape_file="$1"
    awk -v cond="@${TERMINAL_TYPE}" \
        '$1 == "screenshot" { print $2 }
         $1 == cond && $2 == "screenshot" { print $3 }' "$tape_file" 2>/dev/null
}

is_default_excluded_tape() {
    local tape_name="$1"
    for excluded in "${EXCLUDED_DEFAULT_TAPES[@]}"; do
        if [ "$tape_name" = "$excluded" ]; then
            return 0
        fi
    done
    return 1
}

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
    echo "  --memory-leak-limit MB   Fail if memory leak exceeds MB (default: 200)"
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
        --accept)
            ACCEPT_MODE=true
            shift
            ;;
        --screenshot)
            SPECIFIC_SCREENSHOT="$2"
            shift 2
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
            excluded_label=""
            if is_default_excluded_tape "$name"; then
                excluded_label=" [excluded by default]"
            fi
            echo "  $name ($screenshot_count screenshots, $cmd_count commands)$excluded_label"
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

# Accept mode does not launch a terminal or build — it only copies already
# captured screenshots into golden. Skip all terminal prerequisites for it.
if ! $ACCEPT_MODE; then
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
fi

# Create output directories
mkdir -p "$SCREENSHOTS_DIR"
mkdir -p "$REPORTS_DIR"
mkdir -p "$GOLDEN_DIR"

# Golden screenshots live in a separate repo (gitignored here), so a fresh
# checkout of bookokrat has an empty vhs_tests/golden. Comparing against
# nothing would report every screenshot as missing - catch it up front.
# Update/accept modes create goldens, so they're exempt.
if ! $UPDATE_MODE && ! $ACCEPT_MODE; then
    if ! find "$GOLDEN_DIR" -name "*.png" -type f 2>/dev/null | head -1 | grep -q .; then
        echo -e "${RED}ERROR: no golden snapshots found in $GOLDEN_DIR${NC}"
        echo "Goldens live in a separate repo. Clone it first:"
        echo "  git clone https://github.com/bugzmanov/tests-bookokrat-snapshots vhs_tests/golden"
        exit 1
    fi
fi

# A tape may declare `terminal <type>` to restrict itself to one terminal
# (e.g. pdf_dual_wezterm only makes sense on wezterm, pdf_halfblocks_blocked
# relies on the kitty launcher's appenv support). Returns 0 if the tape is
# allowed on the current terminal.
tape_matches_terminal() {
    local tape_file="$1"
    local wanted
    wanted=$(grep -E '^terminal[[:space:]]' "$tape_file" | head -1 | awk '{print $2}')
    [ -z "$wanted" ] || [ "$wanted" = "$TERMINAL_TYPE" ]
}

# Collect tapes to run
tapes_to_run=()
if [ -n "$SPECIFIC_TAPE" ]; then
    tape_file="$TAPES_DIR/$SPECIFIC_TAPE.tape"
    if [ ! -f "$tape_file" ]; then
        echo -e "${RED}ERROR: Tape not found: $tape_file${NC}"
        echo "Use --list to see available tapes"
        exit 1
    fi
    if ! tape_matches_terminal "$tape_file"; then
        echo -e "${RED}ERROR: Tape $SPECIFIC_TAPE is restricted to terminal '$(grep -E '^terminal[[:space:]]' "$tape_file" | head -1 | awk '{print $2}')' (current: $TERMINAL_TYPE)${NC}"
        exit 1
    fi
    tapes_to_run+=("$tape_file")
else
    for tape in "$TAPES_DIR"/*.tape; do
        if [ -f "$tape" ]; then
            tape_name=$(basename "$tape" .tape)
            if ! is_default_excluded_tape "$tape_name" && tape_matches_terminal "$tape"; then
                tapes_to_run+=("$tape")
            fi
        fi
    done
fi

if [ ${#tapes_to_run[@]} -eq 0 ]; then
    echo -e "${YELLOW}No tapes found in $TAPES_DIR${NC}"
    echo "Create a .tape file to get started"
    exit 0
fi

# Accept mode: copy already-captured screenshots into golden, WITHOUT re-running
# tapes. Accepts all screenshots of the selected tape(s), or just one with
# --screenshot. This is what the report's per-snapshot "Accept" buttons call.
if $ACCEPT_MODE; then
    accepted_total=0
    for tape_file in "${tapes_to_run[@]}"; do
        tape_name=$(basename "$tape_file" .tape)
        src_dir="$SCREENSHOTS_DIR/$TERMINAL_TYPE/$tape_name"
        dst_dir="$GOLDEN_DIR/$TERMINAL_TYPE/$tape_name"
        if [ -n "$SPECIFIC_SCREENSHOT" ]; then
            shots=("$SPECIFIC_SCREENSHOT")
        else
            shots=($(tape_screenshot_names "$tape_file"))
        fi
        [ ${#shots[@]} -eq 0 ] && continue
        echo -e "${YELLOW}Accepting golden(s) for $tape_name ($TERMINAL_TYPE)...${NC}"
        update_golden_snapshots "$src_dir" "$dst_dir" "${shots[@]}"
        accepted_total=$((accepted_total + ${#shots[@]}))
    done
    echo -e "${GREEN}✓ Accepted $accepted_total snapshot(s) into golden${NC}"
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
ran_tapes=()   # tape names that produced screenshots (for the aggregate report)

for tape_file in "${tapes_to_run[@]}"; do
    tape_name=$(basename "$tape_file" .tape)
    tape_screenshots_dir="$SCREENSHOTS_DIR/$TERMINAL_TYPE/$tape_name"
    tape_golden_dir="$GOLDEN_DIR/$TERMINAL_TYPE/$tape_name"

    mkdir -p "$tape_screenshots_dir"
    mkdir -p "$tape_golden_dir"

    # Kitty: give each tape its OWN fresh instance. In a shared instance the
    # previous tape's window can linger/occlude, so the next tape's window opens
    # not-frontmost and macOS pauses its GPU graphics -> blank PDF capture. A
    # fresh instance per tape matches the reliable single-tape path.
    if [ "$TERMINAL_TYPE" = "kitty" ]; then
        cleanup_kitty 2>/dev/null || true
        sleep 0.5
        check_kitty || { echo -e "${RED}Kitty relaunch failed for $tape_name${NC}"; continue; }
    fi

    # Run the tape (|| true prevents set -e from exiting on tape errors)
    run_tape "$tape_file" "$BINARY" "$TEST_PDF" "$tape_screenshots_dir" || true

    # Get screenshots that were taken (parse from tape file)
    screenshots=($(tape_screenshot_names "$tape_file"))

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
        ran_tapes+=("$tape_name")
    fi
done

# One aggregate report for all tapes (grouped by scenario, images by path).
if ! $UPDATE_MODE && [ ${#ran_tapes[@]} -gt 0 ]; then
    echo ""
    echo "Generating aggregate report..."
    aggregate_report="$REPORTS_DIR/${TERMINAL_TYPE}_report.html"
    if generate_aggregate_report "$TERMINAL_TYPE" "$aggregate_report" \
        "$GOLDEN_DIR" "$SCREENSHOTS_DIR" "$TAPES_DIR" "${ran_tapes[@]}"; then
        total_passed=${#ran_tapes[@]}
    else
        total_failed=1
    fi
    reports_generated+=("$aggregate_report")
fi

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
