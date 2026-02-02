#!/usr/bin/env bash
#
# wezterm_manager.sh - WezTerm terminal management for VHS tests
#
# Uses WezTerm's CLI (wezterm cli) for reliable remote control.
# Similar to Kitty, WezTerm can be controlled without stealing focus.
#
# Requirements:
#   - WezTerm installed (checks common locations)

# Global state
WEZTERM_PANE_ID=""
WEZTERM_PID=""
WEZTERM_CMD=""
WEZTERM_MACOS_WINDOW_ID=""
WEZTERM_MANAGED=false

# Find wezterm executable
find_wezterm() {
    # Check PATH first
    if command -v wezterm &>/dev/null; then
        WEZTERM_CMD="wezterm"
        return 0
    fi

    # Check macOS app bundle in Applications
    if [ -x "/Applications/WezTerm.app/Contents/MacOS/wezterm" ]; then
        WEZTERM_CMD="/Applications/WezTerm.app/Contents/MacOS/wezterm"
        return 0
    fi

    # Check Downloads (common for testing)
    local downloads_wezterm=$(find ~/Downloads -maxdepth 2 -name "WezTerm.app" -type d 2>/dev/null | head -1)
    if [ -n "$downloads_wezterm" ] && [ -x "$downloads_wezterm/Contents/MacOS/wezterm" ]; then
        WEZTERM_CMD="$downloads_wezterm/Contents/MacOS/wezterm"
        return 0
    fi

    # Check Homebrew
    if [ -x "/opt/homebrew/bin/wezterm" ]; then
        WEZTERM_CMD="/opt/homebrew/bin/wezterm"
        return 0
    fi

    return 1
}

# Check if WezTerm is available
check_wezterm() {
    if [[ "$(uname)" != "Darwin" ]]; then
        echo "ERROR: VHS tests only run on macOS" >&2
        return 1
    fi

    if ! find_wezterm; then
        echo "ERROR: WezTerm not found" >&2
        echo "Install from: https://wezfurlong.org/wezterm/install/macos.html" >&2
        return 1
    fi

    echo "WezTerm found: $WEZTERM_CMD"
    return 0
}

# Get macOS window ID for WezTerm by title
get_wezterm_macos_window_id() {
    local title="$1"
    swift -e "
import Cocoa
let searchTitle = \"$title\"
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == \"wezterm\",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int {
        let name = window[kCGWindowName as String] as? String ?? \"\"
        if searchTitle.isEmpty || name.contains(searchTitle) {
            print(id)
            exit(0)
        }
    }
}
exit(1)
" 2>/dev/null
}

# Get any WezTerm window ID
get_any_wezterm_macos_window_id() {
    swift -e '
import Cocoa
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == "wezterm",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int {
        print(id)
        exit(0)
    }
}
exit(1)
' 2>/dev/null
}

# Launch WezTerm with a command and return the pane ID
# Usage: launch_wezterm "WINDOW_TITLE" "/path/to/command" "args"
# Sets: WEZTERM_PANE_ID, WEZTERM_PID, WEZTERM_MACOS_WINDOW_ID
launch_wezterm() {
    local title="$1"
    local command="$2"
    local args="$3"

    # Ensure we have the wezterm command
    [ -z "$WEZTERM_CMD" ] && find_wezterm

    # Launch WezTerm with large initial size
    # Note: Must redirect stdin/stdout/stderr and disown to prevent subshell from waiting
    "$WEZTERM_CMD" \
        --config initial_cols=250 \
        --config initial_rows=70 \
        start \
        --always-new-process \
        --class "VHS_TEST" \
        -- "$command" $args </dev/null >/dev/null 2>&1 &
    WEZTERM_PID=$!
    disown $WEZTERM_PID 2>/dev/null || true
    WEZTERM_MANAGED=true

    # Wait for window to appear
    sleep 3

    # Skip pane ID detection - we use AppleScript for keystrokes, not wezterm cli
    # (CLI requires shared socket which --always-new-process doesn't provide)
    WEZTERM_PANE_ID=""

    # Get the macOS window ID for screenshots
    WEZTERM_MACOS_WINDOW_ID=$(get_any_wezterm_macos_window_id || true)

    # Return placeholder (we use AppleScript, not pane targeting)
    echo "wezterm"
}

# Send a keystroke to WezTerm using AppleScript (most reliable for isolated processes)
# Usage: send_wezterm_key "j" or send_wezterm_key "?"
send_wezterm_key() {
    local key="$1"

    # Handle special key names
    if [ "$key" = "space" ]; then
        osascript -e '
tell application "WezTerm" to activate
delay 0.1
tell application "System Events"
    key code 49
end tell
' 2>/dev/null
        return
    fi

    osascript -e "
tell application \"WezTerm\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\"
end tell
" 2>/dev/null
}

# Send Ctrl+key to WezTerm using AppleScript
# Usage: send_wezterm_ctrl_key "z"
send_wezterm_ctrl_key() {
    local key="$1"
    osascript -e "
tell application \"WezTerm\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\" using control down
end tell
" 2>/dev/null
}

# Send Escape key (key code 53)
send_wezterm_escape() {
    osascript -e '
tell application "WezTerm" to activate
delay 0.1
tell application "System Events"
    key code 53
end tell
' 2>/dev/null
}

# Send Return/Enter key (key code 36)
send_wezterm_return() {
    osascript -e '
tell application "WezTerm" to activate
delay 0.1
tell application "System Events"
    key code 36
end tell
' 2>/dev/null
}

# Send Tab key (key code 48)
send_wezterm_tab() {
    osascript -e '
tell application "WezTerm" to activate
delay 0.1
tell application "System Events"
    key code 48
end tell
' 2>/dev/null
}

# Send Shift+Tab (key code 48 with shift)
send_wezterm_shift_tab() {
    osascript -e '
tell application "WezTerm" to activate
delay 0.1
tell application "System Events"
    key code 48 using shift down
end tell
' 2>/dev/null
}

# Close the WezTerm window
close_wezterm() {
    # Send 'q' to quit the app first
    send_wezterm_key "q"
    sleep 1.5

    # Force kill if still running (by PID if available)
    if [ -n "$WEZTERM_PID" ]; then
        if kill -0 "$WEZTERM_PID" 2>/dev/null; then
            kill -9 "$WEZTERM_PID" 2>/dev/null || true
        fi
    fi

    # Fallback: kill wezterm-gui processes
    # (handles case where PID was lost due to subshell)
    pkill -9 -f "wezterm-gui" 2>/dev/null || true

    # Reset globals
    WEZTERM_PANE_ID=""
    WEZTERM_MACOS_WINDOW_ID=""
}

# Cleanup managed WezTerm instance (call at end of test session)
cleanup_wezterm() {
    if [ -n "$WEZTERM_PID" ]; then
        echo "Terminating managed WezTerm instance..."
        kill -9 "$WEZTERM_PID" 2>/dev/null || true
    fi

    # Fallback: kill wezterm-gui processes
    pkill -9 -f "wezterm-gui" 2>/dev/null || true

    WEZTERM_PID=""
    WEZTERM_MANAGED=false
    WEZTERM_PANE_ID=""
    WEZTERM_MACOS_WINDOW_ID=""
}
