#!/usr/bin/env bash
#
# kitty_manager.sh - Kitty terminal management for VHS tests
#
# Uses Kitty's remote control (kitty @) for reliable window targeting.
# This allows tests to run in background without stealing focus!
#
# Requirements:
#   - Kitty with remote control enabled (allow_remote_control yes in kitty.conf)
#   - Or launch kitty with: kitty -o allow_remote_control=yes

# Global state
KITTY_WINDOW_ID=""
KITTY_OS_WINDOW_ID=""
KITTY_MACOS_WINDOW_ID=""  # macOS CGWindowNumber for screencapture
KITTY_CMD=""
KITTY_SOCKET=""
KITTY_PID=""
KITTY_MANAGED=false  # True if we launched our own Kitty instance

# Find kitty executable
find_kitty() {
    # Check PATH first
    if command -v kitty &>/dev/null; then
        KITTY_CMD="kitty"
        return 0
    fi

    # Check macOS app bundle
    if [ -x "/Applications/kitty.app/Contents/MacOS/kitty" ]; then
        KITTY_CMD="/Applications/kitty.app/Contents/MacOS/kitty"
        return 0
    fi

    # Check Homebrew
    if [ -x "/opt/homebrew/bin/kitty" ]; then
        KITTY_CMD="/opt/homebrew/bin/kitty"
        return 0
    fi

    return 1
}

# Check if Kitty is available and start a managed instance for testing
check_kitty() {
    if ! find_kitty; then
        echo "ERROR: kitty not found" >&2
        return 1
    fi

    # Socket base name (Kitty appends the PID to the name)
    local socket_base="/tmp/kitty-vhs-test-$$"

    # Launch our own Kitty instance with remote control enabled
    echo "Launching managed Kitty instance..."
    "$KITTY_CMD" \
        -o allow_remote_control=yes \
        -o "listen_on=unix:$socket_base" \
        -o confirm_os_window_close=0 \
        --title "VHS_TEST_KITTY" \
        &
    KITTY_PID=$!
    KITTY_MANAGED=true

    # Wait for socket to appear (Kitty appends its PID to the socket name)
    local attempts=0
    local actual_socket=""
    while [ $attempts -lt 30 ]; do
        # Find the actual socket file (with Kitty's PID appended)
        actual_socket=$(ls ${socket_base}-* 2>/dev/null | head -1)
        if [ -n "$actual_socket" ] && [ -S "$actual_socket" ]; then
            KITTY_SOCKET="unix:$actual_socket"
            if "$KITTY_CMD" @ --to "$KITTY_SOCKET" ls &>/dev/null; then
                echo "Kitty ready (socket: $KITTY_SOCKET)"
                return 0
            fi
        fi
        sleep 0.2
        attempts=$((attempts + 1))
    done

    echo "ERROR: Kitty failed to start with remote control" >&2
    kill "$KITTY_PID" 2>/dev/null || true
    return 1
}

# Launch Kitty with a command and return the window ID
# Usage: launch_kitty "WINDOW_TITLE" "/path/to/command" "args"
# Sets: KITTY_WINDOW_ID, KITTY_OS_WINDOW_ID, KITTY_MACOS_WINDOW_ID
launch_kitty() {
    local title="$1"
    local command="$2"
    local args="$3"

    # Ensure we have the kitty command
    [ -z "$KITTY_CMD" ] && find_kitty

    # Check if there's an existing window, create one if not
    local existing_window=$("$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
for os_win in data:
    for tab in os_win.get('tabs', []):
        for win in tab.get('windows', []):
            print(win.get('id'))
            sys.exit(0)
print('')
" 2>/dev/null)

    if [ -z "$existing_window" ]; then
        # Create a new window (returns the window ID)
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" new-window --title "$title" >/dev/null
        # Wait for shell to initialize in new window
        sleep 1.5
    else
        # Set the window title
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" set-window-title "$title" >/dev/null
    fi

    # Get the window ID first so we can target send-text
    KITTY_WINDOW_ID=$("$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
for os_win in data:
    for tab in os_win.get('tabs', []):
        for win in tab.get('windows', []):
            print(win.get('id'))
            sys.exit(0)
" 2>/dev/null)

    # Send the command to run in the window (with explicit match)
    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-text --match "id:$KITTY_WINDOW_ID" "$command $args\r"
    else
        echo "ERROR: No window ID for send-text" >&2
        return 1
    fi

    # Window ID already set above
    if [ -z "$KITTY_WINDOW_ID" ]; then
        echo "ERROR: Failed to get Kitty window ID" >&2
        return 1
    fi

    # Wait for app to start rendering
    sleep 3

    # Get the macOS window ID (there's only one Kitty window)
    KITTY_MACOS_WINDOW_ID=$(get_any_kitty_macos_window_id)

    echo "$KITTY_WINDOW_ID"
}

# Capture screenshot of Kitty window
# Kitty's screenshot command saves to a file
capture_kitty_screenshot() {
    local output_path="$1"

    if [ -n "$KITTY_WINDOW_ID" ]; then
        # Use kitty's built-in screenshot (captures the specific window)
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" screenshot --match "id:$KITTY_WINDOW_ID" "$output_path" 2>/dev/null
    fi
}

# Send a keystroke to specific Kitty window
# Usage: send_kitty_key "j" or send_kitty_key "?"
send_kitty_key() {
    local key="$1"

    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "$key" 2>/dev/null
    else
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send key: $key" >&2
    fi
}

# Send text to specific Kitty window
send_kitty_text() {
    local text="$1"

    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-text --match "id:$KITTY_WINDOW_ID" "$text" 2>/dev/null
    fi
}

# Send Ctrl+key to Kitty window
send_kitty_ctrl_key() {
    local key="$1"

    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "ctrl+$key" 2>/dev/null
    else
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send ctrl+$key" >&2
    fi
}

# Send Escape key
send_kitty_escape() {
    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "escape" 2>/dev/null
    else
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send escape" >&2
    fi
}

# Send Return key
send_kitty_return() {
    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "enter" 2>/dev/null
    else
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send enter" >&2
    fi
}

# Close the Kitty window
close_kitty() {
    # Send 'q' to quit the app first
    send_kitty_key "q"
    sleep 0.5

    # Close the window
    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" close-window --match "id:$KITTY_WINDOW_ID" 2>/dev/null || true
    fi

    # Reset window globals
    KITTY_WINDOW_ID=""
    KITTY_OS_WINDOW_ID=""
}

# Cleanup managed Kitty instance (call at end of test session)
cleanup_kitty() {
    if $KITTY_MANAGED && [ -n "$KITTY_PID" ]; then
        echo "Terminating managed Kitty instance..."
        # Force kill immediately (graceful quit can hang)
        kill -9 "$KITTY_PID" 2>/dev/null || true
        # Clean up socket file
        if [ -n "$KITTY_SOCKET" ]; then
            local socket_path="${KITTY_SOCKET#unix:}"
            rm -f "$socket_path" 2>/dev/null || true
        fi
    fi

    KITTY_PID=""
    KITTY_MANAGED=false
    KITTY_SOCKET=""
}

# Get ALL macOS window IDs for Kitty (one per line)
get_all_kitty_macos_window_ids() {
    swift -e '
import Cocoa
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == "kitty",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int {
        print(id)
    }
}
' 2>/dev/null
}

# Get macOS window ID for Kitty by title (or any Kitty window if title is empty)
get_kitty_macos_window_id() {
    local title="$1"
    swift -e "
import Cocoa
let searchTitle = \"$title\"
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == \"kitty\",
       let layer = window[kCGWindowLayer as String] as? Int,
       layer == 0,
       let id = window[kCGWindowNumber as String] as? Int {
        let name = window[kCGWindowName as String] as? String ?? \"\"
        // Match if no search title, or title contains search string
        if searchTitle.isEmpty || name.contains(searchTitle) {
            print(id)
            exit(0)
        }
    }
}
exit(1)
" 2>/dev/null
}

# Get any Kitty window ID (for when title matching fails)
get_any_kitty_macos_window_id() {
    swift -e '
import Cocoa
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly)
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for window in windowList {
    if let owner = window[kCGWindowOwnerName as String] as? String,
       owner.lowercased() == "kitty",
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
