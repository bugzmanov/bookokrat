#!/usr/bin/env bash
#
# window_manager.sh - Ghostty window management for macOS
#
# Provides functions for launching, capturing, and closing Ghostty windows.
# Requires macOS with Ghostty.app installed.
#
# Similar to kitty_manager.sh, we track our Ghostty process for reliable cleanup.

# Global state
GHOSTTY_WINDOW_ID=""
GHOSTTY_WINDOW_TITLE=""
GHOSTTY_PID=""          # PID of Ghostty process we launched
GHOSTTY_PID_FILE=""     # PID file for shell process
GHOSTTY_TEMP_SCRIPT=""  # Temp script file
GHOSTTY_MANAGED=false   # True if we launched a managed instance

# Get all Ghostty window IDs currently on screen
get_all_ghostty_window_ids() {
    swift -e '
import Cocoa
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == "ghostty",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int {
        print(id)
    }
}
' 2>/dev/null
}

# Get Ghostty window ID by title substring
get_window_id_by_title() {
    local search_title="$1"
    swift -e "
import Cocoa
let searchTitle = \"$search_title\"
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == \"ghostty\",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int,
       let name = window[kCGWindowName as String] as? String {
        if searchTitle.isEmpty || name.contains(searchTitle) {
            print(id)
            exit(0)
        }
    }
}
exit(1)
" 2>/dev/null
}

# Launch Ghostty with a command and return the new window ID
# Usage: launch_ghostty "WINDOW_TITLE" "/path/to/command" "args"
# Sets: GHOSTTY_WINDOW_ID, GHOSTTY_PID, GHOSTTY_PID_FILE, GHOSTTY_TEMP_SCRIPT
launch_ghostty() {
    local title="$1"
    local command="$2"
    local args="$3"

    # Record existing window IDs before launching
    local before_ids=$(get_all_ghostty_window_ids | sort)

    # Create temp script that writes its PID and runs the command
    GHOSTTY_TEMP_SCRIPT=$(mktemp)
    GHOSTTY_PID_FILE="${GHOSTTY_TEMP_SCRIPT}.pid"

    cat > "$GHOSTTY_TEMP_SCRIPT" << SCRIPT
#!/bin/bash
echo \$\$ > "$GHOSTTY_PID_FILE"
"$command" $args
# Keep shell open briefly for final capture
sleep 2
SCRIPT
    chmod +x "$GHOSTTY_TEMP_SCRIPT"

    # Launch Ghostty via open (required on macOS)
    # --maximize=true: Open window maximized (fixes small window issue)
    # --window-save-state=never: Prevent restoring previous session
    # --confirm-close-surface=false: Don't prompt when closing
    open -na Ghostty --args \
        --maximize=true \
        --window-save-state=never \
        --confirm-close-surface=false \
        --title="$title" \
        -e "$GHOSTTY_TEMP_SCRIPT"

    # Wait for window to appear
    sleep 3

    # Get the Ghostty process PID (most recent Ghostty process)
    GHOSTTY_PID=$(pgrep -n ghostty 2>/dev/null || true)
    GHOSTTY_MANAGED=true

    # Find the NEW window ID
    local after_ids=$(get_all_ghostty_window_ids | sort)
    GHOSTTY_WINDOW_ID=""

    for id in $after_ids; do
        if ! echo "$before_ids" | grep -q "^${id}$"; then
            GHOSTTY_WINDOW_ID="$id"
            break
        fi
    done

    # Fallback: try by title
    if [ -z "$GHOSTTY_WINDOW_ID" ]; then
        GHOSTTY_WINDOW_ID=$(get_window_id_by_title "$title")
    fi

    if [ -z "$GHOSTTY_WINDOW_ID" ]; then
        echo "ERROR: Could not find new Ghostty window" >&2
        return 1
    fi

    # Store window title for targeting keystrokes to correct window
    GHOSTTY_WINDOW_TITLE="$title"

    echo "$GHOSTTY_WINDOW_ID"
}

# Capture screenshot of a window
# Usage: capture_screenshot WINDOW_ID OUTPUT_PATH
capture_screenshot() {
    local window_id="$1"
    local output_path="$2"

    # screencapture -l captures by window ID, works even if window is in background
    screencapture -l"$window_id" -x -o "$output_path" 2>/dev/null
}

# Send a keystroke to the test window
# Note: Unfortunately, macOS AppleScript can't reliably target specific Ghostty windows.
# We have to activate the app, which brings the most recently launched window to front.
send_key() {
    local key="$1"

    # Handle special key names
    if [ "$key" = "space" ]; then
        osascript -e '
tell application "Ghostty" to activate
delay 0.1
tell application "System Events"
    key code 49
end tell
' 2>/dev/null
        return
    fi

    osascript -e "
tell application \"Ghostty\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\"
end tell
" 2>/dev/null
}

# Send Ctrl+key to Ghostty
# Usage: send_ctrl_key "z"
send_ctrl_key() {
    local key="$1"
    osascript -e "
tell application \"Ghostty\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\" using control down
end tell
" 2>/dev/null
}

# Send a special key by key code
# Common codes: 53=Escape, 36=Return, 123=Left, 124=Right, 125=Down, 126=Up
send_key_code() {
    local key_code="$1"
    osascript -e "
tell application \"Ghostty\" to activate
delay 0.1
tell application \"System Events\"
    key code $key_code
end tell
" 2>/dev/null
}

# Send Escape key
send_escape() {
    send_key_code 53
}

# Send Return key
send_return() {
    send_key_code 36
}

# Send Tab key (key code 48)
send_tab() {
    send_key_code 48
}

# Send Shift+Tab key
send_shift_tab() {
    osascript -e '
tell application "Ghostty" to activate
delay 0.1
tell application "System Events"
    key code 48 using shift down
end tell
' 2>/dev/null
}

# Close the Ghostty window gracefully
# Usage: close_ghostty
close_ghostty() {
    # Send 'q' to quit the app
    send_key "q"
    sleep 1.0

    # Kill the shell process
    if [ -n "$GHOSTTY_PID_FILE" ] && [ -f "$GHOSTTY_PID_FILE" ]; then
        local shell_pid=$(cat "$GHOSTTY_PID_FILE")
        if [ -n "$shell_pid" ]; then
            pkill -P "$shell_pid" 2>/dev/null || true
            kill "$shell_pid" 2>/dev/null || true
        fi
        rm -f "$GHOSTTY_PID_FILE"
    fi

    # Clean up temp script
    if [ -n "$GHOSTTY_TEMP_SCRIPT" ] && [ -f "$GHOSTTY_TEMP_SCRIPT" ]; then
        rm -f "$GHOSTTY_TEMP_SCRIPT"
    fi

    # Wait a bit for graceful close
    sleep 0.5

    # Check if the window is still there, if so force kill
    if [ -n "$GHOSTTY_WINDOW_ID" ]; then
        local still_exists=$(get_window_id_by_title "$GHOSTTY_WINDOW_TITLE" 2>/dev/null || true)
        if [ -n "$still_exists" ]; then
            # Window didn't close gracefully, force kill
            if [ -n "$GHOSTTY_PID" ] && $GHOSTTY_MANAGED; then
                kill -9 "$GHOSTTY_PID" 2>/dev/null || true
            fi
        fi
    fi

    # Reset globals
    GHOSTTY_WINDOW_ID=""
    GHOSTTY_WINDOW_TITLE=""
    GHOSTTY_PID=""
    GHOSTTY_PID_FILE=""
    GHOSTTY_TEMP_SCRIPT=""
    GHOSTTY_MANAGED=false
}

# Cleanup managed Ghostty instance (call at end of test session)
cleanup_ghostty() {
    if $GHOSTTY_MANAGED && [ -n "$GHOSTTY_PID" ]; then
        echo "Terminating managed Ghostty instance..."
        kill -9 "$GHOSTTY_PID" 2>/dev/null || true
    fi

    # Clean up any leftover temp files
    if [ -n "$GHOSTTY_PID_FILE" ] && [ -f "$GHOSTTY_PID_FILE" ]; then
        rm -f "$GHOSTTY_PID_FILE"
    fi
    if [ -n "$GHOSTTY_TEMP_SCRIPT" ] && [ -f "$GHOSTTY_TEMP_SCRIPT" ]; then
        rm -f "$GHOSTTY_TEMP_SCRIPT"
    fi

    GHOSTTY_PID=""
    GHOSTTY_MANAGED=false
}

# Check if Ghostty is available
check_ghostty() {
    if [[ "$(uname)" != "Darwin" ]]; then
        echo "ERROR: VHS tests only run on macOS" >&2
        return 1
    fi

    if ! [ -d "/Applications/Ghostty.app" ]; then
        echo "ERROR: Ghostty.app not found in /Applications" >&2
        return 1
    fi

    return 0
}
