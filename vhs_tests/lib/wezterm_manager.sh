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

    # Stamp file: socket discovery only trusts gui sockets created after this
    # moment, so we can never latch onto the user's own running wezterm.
    WEZTERM_LAUNCH_STAMP="${SCRATCHPAD:-/tmp}/wezterm_launch_stamp_$$"
    touch "$WEZTERM_LAUNCH_STAMP"

    # Launch WezTerm with large initial size
    # Note: Must redirect stdin/stdout/stderr and disown to prevent subshell from waiting
    "$WEZTERM_CMD" \
        --config initial_cols=250 \
        --config initial_rows=70 \
        start \
        --always-new-process \
        --class "VHS_TEST" \
        --cwd "$PWD" \
        -- env -u TMUX -u TMUX_PANE -u GHOSTTY_RESOURCES_DIR -u GHOSTTY_BIN_DIR ${TAPE_APP_ENV:-} "$command" $args </dev/null >/dev/null 2>&1 &
    WEZTERM_PID=$!
    disown $WEZTERM_PID 2>/dev/null || true
    WEZTERM_MANAGED=true

    # Wait for window to appear
    sleep 3

    # NOTE: term_launch runs this inside $(...), so nothing assigned here
    # survives for the caller. Socket discovery for mouse injection happens
    # lazily in _wezterm_ensure_socket (the stamp file path is derived from $$,
    # which is stable across subshells, so it can be reconstructed).
    WEZTERM_PANE_ID=""
    WEZTERM_CELL_W=""; WEZTERM_CELL_H=""

    # Get the macOS window ID for screenshots
    WEZTERM_MACOS_WINDOW_ID=$(get_any_wezterm_macos_window_id || true)

    # Return placeholder (we use AppleScript, not pane targeting)
    echo "wezterm"
}

# ─── Mouse injection ────────────────────────────────────────────────────────
#
# Same approach as the Kitty harness: write the exact SGR mouse escape sequence
# into the app's pty — here via `wezterm cli send-text --no-paste`. The app on
# WezTerm never enables SGR-pixel mode (?1016 is Kitty/Ghostty only), so it
# always expects 1-based CELL coordinates (SGR 1006):
#   - cell tape commands (click/drag/...):  sent as-is.
#   - *px tape commands (clickpx/...):      device pixels converted to the
#     containing cell via a one-time capture-size / grid calibration. Sub-cell
#     precision is lost (the app can't receive pixels), but px-coordinate tapes
#     still hit the right cell.

WEZTERM_SOCK=""
WEZTERM_LAUNCH_STAMP=""
WEZTERM_COLS=""
WEZTERM_ROWS=""
WEZTERM_PANE_PX_W=""
WEZTERM_PANE_PX_H=""
WEZTERM_CELL_W=""
WEZTERM_CELL_H=""
WEZTERM_OFF_X=""
WEZTERM_OFF_Y=""

# Find the gui socket of the wezterm we launched and the (sole) pane in it.
# Each wezterm-gui creates $HOME/.local/share/wezterm/gui-sock-<pid> and the
# class symlink default-VHS_TEST. The user may have their OWN wezterm running,
# so only accept sockets created after our launch (WEZTERM_LAUNCH_STAMP) —
# injecting mouse events into the user's terminal would be very bad.
wezterm_discover_socket() {
    # launch_wezterm ran in a subshell, so re-derive the stamp path ($$ is the
    # same in subshells).
    local stamp="${WEZTERM_LAUNCH_STAMP:-${SCRATCHPAD:-/tmp}/wezterm_launch_stamp_$$}"
    [ -f "$stamp" ] || stamp=""
    local runtime_dir="${XDG_RUNTIME_DIR:-$HOME/.local/share/wezterm}"
    [ -d "$runtime_dir/wezterm" ] && runtime_dir="$runtime_dir/wezterm"
    local candidates=()
    [ -e "$runtime_dir/default-VHS_TEST" ] && candidates+=("$runtime_dir/default-VHS_TEST")
    if [ -n "$stamp" ]; then
        # gui-sock-* scanning is only safe when we can prove the socket is
        # newer than our launch; without the stamp, trust only the VHS_TEST
        # class symlink.
        local s
        for s in $(/bin/ls -t "$runtime_dir"/gui-sock-* 2>/dev/null); do
            candidates+=("$s")
        done
    fi
    local sock info
    for sock in "${candidates[@]}"; do
        if [ -n "$stamp" ] && [ ! "$sock" -nt "$stamp" ]; then
            continue
        fi
        info=$(WEZTERM_UNIX_SOCKET="$sock" "$WEZTERM_CMD" cli list --format json 2>/dev/null)
        if [ -n "$info" ]; then
            WEZTERM_SOCK="$sock"
            read -r WEZTERM_PANE_ID WEZTERM_COLS WEZTERM_ROWS WEZTERM_PANE_PX_W WEZTERM_PANE_PX_H <<< "$(echo "$info" | python3 -c "
import sys, json
panes = json.load(sys.stdin)
if panes:
    p = panes[0]
    s = p['size']
    print(p['pane_id'], s['cols'], s['rows'], s.get('pixel_width', 0), s.get('pixel_height', 0))
")"
            if [ -n "$WEZTERM_PANE_ID" ]; then
                return 0
            fi
        fi
    done
    echo "WARNING: no live VHS wezterm gui socket found; mouse commands will not work" >&2
    return 1
}

# Lazy socket/pane discovery (see launch_wezterm note about subshells).
_wezterm_ensure_socket() {
    [ -n "$WEZTERM_PANE_ID" ] && [ -n "$WEZTERM_SOCK" ] && return 0
    [ -z "$WEZTERM_CMD" ] && find_wezterm
    wezterm_discover_socket
}

# Compute cell pixel size and chrome offsets once. Unlike the kitty window
# (no decorations, capture == grid*cell), the wezterm capture includes the
# title bar + tab bar, so *px tape coordinates (capture space) must be shifted
# by the chrome offset before dividing by the cell size. The pane's true pixel
# dims come from `cli list`; offset = capture size - pane size (chrome is on
# top, pane is bottom-left anchored).
wezterm_calibrate_cell_size() {
    [ -n "$WEZTERM_CELL_W" ] && return 0
    _wezterm_ensure_socket || return 1
    if [ -z "$WEZTERM_COLS" ] || [ "$WEZTERM_COLS" = "0" ] \
        || [ -z "$WEZTERM_PANE_PX_W" ] || [ "$WEZTERM_PANE_PX_W" = "0" ]; then
        echo "WARNING: cannot calibrate wezterm cell size (grid=${WEZTERM_COLS}x${WEZTERM_ROWS} pane_px=${WEZTERM_PANE_PX_W}x${WEZTERM_PANE_PX_H})" >&2
        return 1
    fi
    WEZTERM_CELL_W=$(echo "scale=4; $WEZTERM_PANE_PX_W / $WEZTERM_COLS" | bc -l)
    WEZTERM_CELL_H=$(echo "scale=4; $WEZTERM_PANE_PX_H / $WEZTERM_ROWS" | bc -l)
    WEZTERM_OFF_X=0
    WEZTERM_OFF_Y=0
    local macos_id="$WEZTERM_MACOS_WINDOW_ID"
    [ -z "$macos_id" ] && macos_id=$(get_any_wezterm_macos_window_id 2>/dev/null)
    if [ -n "$macos_id" ]; then
        local tmp="${SCRATCHPAD:-/tmp}/wezterm_calib_$$.png"
        screencapture -l"$macos_id" -x -o "$tmp" 2>/dev/null
        if [ -s "$tmp" ]; then
            local pxw pxh
            pxw=$(sips -g pixelWidth "$tmp" 2>/dev/null | awk '/pixelWidth/{print $2}')
            pxh=$(sips -g pixelHeight "$tmp" 2>/dev/null | awk '/pixelHeight/{print $2}')
            if [[ "$pxw" =~ ^[0-9]+$ ]]; then
                WEZTERM_OFF_X=$(( (pxw - WEZTERM_PANE_PX_W) > 0 ? (pxw - WEZTERM_PANE_PX_W) / 2 : 0 ))
                WEZTERM_OFF_Y=$(( (pxh - WEZTERM_PANE_PX_H) > 0 ? pxh - WEZTERM_PANE_PX_H - (pxw - WEZTERM_PANE_PX_W) / 2 : 0 ))
            fi
        fi
        rm -f "$tmp"
    fi
    return 0
}

# Map an input coordinate to the SGR cx;cy cell to send. Echoes "cx cy".
# KITTY_MOUSE_RAW_PX is the runner's terminal-agnostic "inputs are device
# pixels" flag (set for the *px tape commands): convert pixel -> containing
# cell. Otherwise inputs are already 1-based cells.
_wezterm_coord_to_sgr() {
    local col="$1" row="$2"
    if [ "${KITTY_MOUSE_RAW_PX:-false}" = true ]; then
        wezterm_calibrate_cell_size || { echo "$col $row"; return; }
        local cx cy
        cx=$(echo "scale=0; (($col - ${WEZTERM_OFF_X:-0}) / $WEZTERM_CELL_W) / 1 + 1" | bc -l)
        cy=$(echo "scale=0; (($row - ${WEZTERM_OFF_Y:-0}) / $WEZTERM_CELL_H) / 1 + 1" | bc -l)
        [ "$cx" -lt 1 ] 2>/dev/null && cx=1
        [ "$cy" -lt 1 ] 2>/dev/null && cy=1
        echo "$cx $cy"
    else
        echo "$col $row"
    fi
}

# Send one raw SGR mouse report (M=press/motion, m=release) into the pane's pty.
_wezterm_mouse_raw() {
    local cb="$1" cx="$2" cy="$3" final="$4"
    if ! _wezterm_ensure_socket; then
        echo "WARNING: no wezterm pane for mouse (socket discovery failed)" >&2
        return 1
    fi
    printf '\033[<%s;%s;%s%s' "$cb" "$cx" "$cy" "$final" \
        | WEZTERM_UNIX_SOCKET="$WEZTERM_SOCK" "$WEZTERM_CMD" cli send-text --no-paste \
            --pane-id "$WEZTERM_PANE_ID" 2>/dev/null
}

# Send raw bytes into the pane's pty (keys). Much faster and more reliable
# than AppleScript keystrokes (whose per-key osascript latency can exceed the
# app's 1s multi-key sequence timeout, silently dropping Space-prefixed
# commands). Args are printf-style: FORMAT [ARGS...].
_wezterm_send_bytes() {
    if ! _wezterm_ensure_socket; then
        return 1
    fi
    printf "$@" | WEZTERM_UNIX_SOCKET="$WEZTERM_SOCK" "$WEZTERM_CMD" cli send-text --no-paste \
        --pane-id "$WEZTERM_PANE_ID" 2>/dev/null
}

_wezterm_button_code() {
    case "$1" in
        right)  echo 2 ;;
        middle) echo 1 ;;
        left|*) echo 0 ;;
    esac
}

# Click (press+release). Usage: send_wezterm_mouse_click COL ROW [button]
send_wezterm_mouse_click() {
    local col="$1" row="$2" button="${3:-left}"
    local cb; cb=$(_wezterm_button_code "$button")
    local sgr; sgr=$(_wezterm_coord_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    _wezterm_mouse_raw "$cb" "$cx" "$cy" "M"
    _wezterm_mouse_raw "$cb" "$cx" "$cy" "m"
}

# Multi-click (2=double, 3=triple), back-to-back so MouseTracker groups them.
send_wezterm_mouse_multiclick() {
    local count="$1" col="$2" row="$3" button="${4:-left}"
    local cb; cb=$(_wezterm_button_code "$button")
    local sgr; sgr=$(_wezterm_coord_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    local i
    for ((i = 0; i < count; i++)); do
        _wezterm_mouse_raw "$cb" "$cx" "$cy" "M"
        _wezterm_mouse_raw "$cb" "$cx" "$cy" "m"
    done
}

# Drag: press, midpoint motion (button-held bit 32), end motion, release.
send_wezterm_mouse_drag() {
    local c1="$1" r1="$2" c2="$3" r2="$4" button="${5:-left}"
    local cb; cb=$(_wezterm_button_code "$button")
    local motion=$((cb + 32))
    local s1 s2 sm
    s1=$(_wezterm_coord_to_sgr "$c1" "$r1")
    s2=$(_wezterm_coord_to_sgr "$c2" "$r2")
    local mcol=$(( (c1 + c2) / 2 )) mrow=$(( (r1 + r2) / 2 ))
    sm=$(_wezterm_coord_to_sgr "$mcol" "$mrow")
    local x1 y1 x2 y2 xm ym
    read -r x1 y1 <<< "$s1"; read -r x2 y2 <<< "$s2"; read -r xm ym <<< "$sm"
    _wezterm_mouse_raw "$cb" "$x1" "$y1" "M"
    _wezterm_mouse_raw "$motion" "$xm" "$ym" "M"
    _wezterm_mouse_raw "$motion" "$x2" "$y2" "M"
    _wezterm_mouse_raw "$cb" "$x2" "$y2" "m"
}

# Scroll wheel. Usage: send_wezterm_mouse_scroll up|down COL ROW [count]
send_wezterm_mouse_scroll() {
    local dir="$1" col="$2" row="$3" count="${4:-1}"
    local cb
    case "$dir" in
        up)   cb=64 ;;
        down) cb=65 ;;
        *) echo "WARNING: scroll dir must be up|down" >&2; return 1 ;;
    esac
    local sgr; sgr=$(_wezterm_coord_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    local i
    for ((i = 0; i < count; i++)); do
        _wezterm_mouse_raw "$cb" "$cx" "$cy" "M"
    done
}

# Mouse move (no button). 35 = motion, no button (3 + 32).
send_wezterm_mouse_move() {
    local col="$1" row="$2"
    local sgr; sgr=$(_wezterm_coord_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    _wezterm_mouse_raw "35" "$cx" "$cy" "M"
}

# Send a keystroke to WezTerm. Prefers raw pty injection over the gui socket
# (fast + deterministic); falls back to AppleScript when the socket is gone.
# Usage: send_wezterm_key "j" or send_wezterm_key "?"
send_wezterm_key() {
    local key="$1"
    local ch="$key"
    [ "$key" = "space" ] && ch=" "
    if _wezterm_send_bytes '%s' "$ch"; then
        return
    fi

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
    # Ctrl+letter = ASCII code of the (lowercase) letter & 0x1f
    local code
    code=$(printf '%d' "'${key}")
    if _wezterm_send_bytes "\\$(printf '%03o' $((code & 31)))"; then
        return
    fi
    osascript -e "
tell application \"WezTerm\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\" using control down
end tell
" 2>/dev/null
}

# Send Ctrl+Shift+key to WezTerm using AppleScript
# Usage: send_wezterm_ctrl_shift_key "z"
send_wezterm_ctrl_shift_key() {
    local key="$1"
    osascript -e "
tell application \"WezTerm\" to activate
delay 0.1
tell application \"System Events\"
    keystroke \"$key\" using {control down, shift down}
end tell
" 2>/dev/null
}

# Send Escape key (key code 53)
send_wezterm_escape() {
    if _wezterm_send_bytes '\033'; then
        return
    fi
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
    if _wezterm_send_bytes '\r'; then
        return
    fi
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
    if _wezterm_send_bytes '\t'; then
        return
    fi
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
    if _wezterm_send_bytes '\033[Z'; then
        return
    fi
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
    # Also kill stray mux-servers: a lingering mux makes the next
    # `wezterm start` delegate to it and spawn the app HEADLESS (no window).
    pkill -9 -f "wezterm-mux-server" 2>/dev/null || true

    # Reset globals
    WEZTERM_PANE_ID=""
    WEZTERM_MACOS_WINDOW_ID=""
    _wezterm_reset_mouse_state
}

# Reset per-launch mouse/socket state so the next tape re-discovers (lazy
# discovery would otherwise reuse a dead socket or stale calibration).
_wezterm_reset_mouse_state() {
    WEZTERM_SOCK=""
    WEZTERM_COLS=""; WEZTERM_ROWS=""
    WEZTERM_PANE_PX_W=""; WEZTERM_PANE_PX_H=""
    WEZTERM_CELL_W=""; WEZTERM_CELL_H=""
    WEZTERM_OFF_X=""; WEZTERM_OFF_Y=""
    [ -n "$WEZTERM_LAUNCH_STAMP" ] && rm -f "$WEZTERM_LAUNCH_STAMP" 2>/dev/null
    WEZTERM_LAUNCH_STAMP=""
}

# Cleanup managed WezTerm instance (call at end of test session)
cleanup_wezterm() {
    if [ -n "$WEZTERM_PID" ]; then
        echo "Terminating managed WezTerm instance..."
        kill -9 "$WEZTERM_PID" 2>/dev/null || true
    fi

    # Fallback: kill wezterm-gui processes
    pkill -9 -f "wezterm-gui" 2>/dev/null || true
    pkill -9 -f "wezterm-mux-server" 2>/dev/null || true

    WEZTERM_PID=""
    WEZTERM_MANAGED=false
    WEZTERM_PANE_ID=""
    WEZTERM_MACOS_WINDOW_ID=""
    _wezterm_reset_mouse_state
}
