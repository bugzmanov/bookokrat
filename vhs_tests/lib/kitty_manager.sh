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
KITTY_SOCKET_BASE=""  # listen_on path without the PID suffix (for cleanup match)
KITTY_PID=""
KITTY_MANAGED=false  # True if we launched our own Kitty instance

# Pinned OS-window size in POINTS (kitty reads initial_window_* as points on
# macOS): 1512x861pt = 3024x1722 device px @2x — the size every kitty golden
# was captured at. Without pinning, remember_window_size makes each launch
# inherit whatever geometry the last kitty session (or a resize tape) left
# behind, and the whole suite fails on dimension mismatch.
# CHANGING THESE INVALIDATES EVERY GOLDEN under vhs_tests/golden/kitty/.
KITTY_PIN_WIDTH_PT=1512
KITTY_PIN_HEIGHT_PT=861

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
    KITTY_SOCKET_BASE="$socket_base"

    # Refuse to run alongside another VHS suite. Its kitty window sits at the
    # same pinned position and occludes ours (or vice versa); macOS stops
    # repainting the hidden window and screencapture then returns stale frames,
    # so both runs report bogus failures. Our own sockets carry this shell's
    # PID (kitty is relaunched per tape), so only foreign, LIVE sockets count;
    # leftovers from a crashed run don't answer `kitty @ ls` and are ignored.
    local other
    for other in /tmp/kitty-vhs-test-*; do
        [ -S "$other" ] || continue
        [[ "$other" == "$socket_base"-* ]] && continue
        if "$KITTY_CMD" @ --to "unix:$other" ls &>/dev/null; then
            echo "ERROR: another VHS kitty instance is running (socket: $other)." >&2
            echo "Two suites on one display occlude each other and capture stale frames." >&2
            echo "Wait for that run to finish, or stop it: pkill -f 'listen_on=unix:${other%-*}'" >&2
            return 1
        fi
    done

    # Resolve the .app bundle so we can launch in the BACKGROUND without stealing
    # focus. `open -g` launches without activating; a bare `kitty &` would steal
    # focus on launch.
    local app="/Applications/kitty.app"
    if [[ "$KITTY_CMD" == *.app/* ]]; then app="${KITTY_CMD%%.app/*}.app"; fi

    echo "Launching managed Kitty instance (background)..."
    # hide_window_decorations=yes removes the macOS title bar so screencapture
    # grabs ONLY kitty's content area. The title-bar height is drawn by the OS
    # (not kitty) and changes across macOS updates; capturing it makes goldens
    # drift by a few pixels on every OS upgrade. Without chrome, the capture is
    # purely rows*cell_height and stays deterministic.
    open -g -n -a "$app" --args \
        -o allow_remote_control=yes \
        -o "listen_on=unix:$socket_base" \
        -o confirm_os_window_close=0 \
        -o hide_window_decorations=yes \
        -o remember_window_size=no \
        -o "initial_window_width=${KITTY_PIN_WIDTH_PT}" \
        -o "initial_window_height=${KITTY_PIN_HEIGHT_PT}" \
        --title "VHS_TEST_KITTY" 2>/dev/null
    KITTY_MANAGED=true

    # Wait for socket to appear (Kitty appends its PID to the socket name)
    local attempts=0
    local actual_socket=""
    while [ $attempts -lt 60 ]; do
        # Find the actual socket file (with Kitty's PID appended)
        actual_socket=$(ls ${socket_base}-* 2>/dev/null | head -1)
        if [ -n "$actual_socket" ] && [ -S "$actual_socket" ]; then
            KITTY_SOCKET="unix:$actual_socket"
            if "$KITTY_CMD" @ --to "$KITTY_SOCKET" ls &>/dev/null; then
                # open(1) detaches, so recover the PID for cleanup.
                KITTY_PID=$(pgrep -f "listen_on=unix:$socket_base" | head -1)
                echo "Kitty ready (socket: $KITTY_SOCKET)"
                return 0
            fi
        fi
        sleep 0.25
        attempts=$((attempts + 1))
    done

    echo "ERROR: Kitty failed to start with remote control" >&2
    pkill -f "listen_on=unix:$socket_base" 2>/dev/null || true
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

    # Run the binary DIRECTLY as the window's program (no login shell). This is
    # critical: going through the shell auto-attaches tmux, which makes bookokrat
    # use tmux graphics passthrough and the PDF fails to render. `env -u TMUX`
    # strips any inherited tmux env for good measure.
    # TAPE_APP_ENV carries `appenv NAME=VALUE` directives from the tape
    # (e.g. BOOKOKRAT_PROTOCOL=halfblocks to force the non-kitty render path).
    "$KITTY_CMD" @ --to "$KITTY_SOCKET" launch --type=os-window --title "$title" \
        --cwd "${PROJECT_ROOT:-$PWD}" \
        env -u TMUX -u TMUX_PANE $TAPE_APP_ENV "$command" $args >/dev/null 2>&1

    # Find the bookokrat window: its kitty id (for send-key) and its os-window's
    # platform_window_id (the macOS CGWindowNumber for screencapture). Match by
    # the binary name in the cmdline; take the newest if several exist.
    # Wait for the bookokrat window AND a valid macOS platform_window_id. The
    # platform id is briefly null right after `launch`, so we must NOT proceed
    # until BOTH are real integers — otherwise capture falls back to "any kitty
    # window" and grabs the (blank) host shell window instead.
    local bin_base; bin_base="$(basename "$command")"
    KITTY_WINDOW_ID=""; KITTY_MACOS_WINDOW_ID=""
    local i ids
    for i in $(seq 1 60); do
        ids=$("$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null | BIN="$bin_base" python3 -c "
import sys, json, os
bin_base = os.environ['BIN']
d = json.load(sys.stdin); best = None
for ow in d:
    pwid = ow.get('platform_window_id')
    for t in ow.get('tabs', []):
        for w in t.get('windows', []):
            if bin_base in ' '.join(w.get('cmdline', [])):
                wid = w.get('id')
                if wid is not None and pwid is not None and (best is None or wid > best[0]):
                    best = (wid, pwid)
if best: print(best[0], best[1])
")
        read -r KITTY_WINDOW_ID KITTY_MACOS_WINDOW_ID <<< "$ids"
        if [[ "$KITTY_WINDOW_ID" =~ ^[0-9]+$ && "$KITTY_MACOS_WINDOW_ID" =~ ^[0-9]+$ ]]; then
            break
        fi
        KITTY_WINDOW_ID=""; KITTY_MACOS_WINDOW_ID=""
        sleep 0.2
    done

    if [[ ! "$KITTY_MACOS_WINDOW_ID" =~ ^[0-9]+$ ]]; then
        echo "ERROR: bookokrat window/platform id not found after launch" >&2
        return 1
    fi

    # Wait for the app to start rendering the first PDF page.
    sleep 3

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

    if [ -z "$KITTY_WINDOW_ID" ]; then
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send key: $key" >&2
        return
    fi

    # `kitty @ send-key` lowercases single letters (send-key D and send-key
    # shift+d both deliver 'd'). For a single uppercase ASCII letter, send the
    # raw byte via send-text so the app receives a real capital (e.g. V for
    # visual-line, D for dual-page toggle, H/L for pan).
    if [[ "$key" =~ ^[A-Z]$ ]]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-text --match "id:$KITTY_WINDOW_ID" "$key" 2>/dev/null
    else
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "$key" 2>/dev/null
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

# Send Ctrl+Shift+key to Kitty window
send_kitty_ctrl_shift_key() {
    local key="$1"

    if [ -n "$KITTY_WINDOW_ID" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-key --match "id:$KITTY_WINDOW_ID" "ctrl+shift+$key" 2>/dev/null
    else
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot send ctrl+shift+$key" >&2
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

# Resize the OS window holding the test window by a signed cell delta.
# Usage: send_kitty_resize_window DCOLS DROWS
# Drives the app's SIGWINCH path (viewport change + re-render), which cell/pixel
# injection cannot. Invalidate the cached mouse calibration: the capture size
# and grid both change, so a later cell->pixel conversion must recalibrate.
send_kitty_resize_window() {
    local dw="$1" dh="${2:-0}"
    if [ -z "$KITTY_WINDOW_ID" ]; then
        echo "WARNING: KITTY_WINDOW_ID is empty, cannot resize" >&2
        return 1
    fi
    "$KITTY_CMD" @ --to "$KITTY_SOCKET" resize-os-window \
        --match "id:$KITTY_WINDOW_ID" --action resize --unit cells \
        --incremental --width="$dw" --height="$dh" 2>/dev/null
    KITTY_CELL_W=""
    KITTY_CELL_H=""
}

# ─── Mouse injection ────────────────────────────────────────────────────────
#
# We do NOT move the OS cursor (cliclick/CGEvent would steal the pointer, need
# Accessibility, and be non-deterministic). Instead we write the exact SGR mouse
# escape sequence the terminal itself would emit straight into the app's pty via
# `kitty @ send-text`. crossterm (and bookokrat's own PDF event parser) read it
# as a genuine mouse event — no cursor, no focus, no permissions.
#
# Coordinate space depends on context:
#   - EPUB / non-Kitty:        cells (SGR 1006). cx;cy are 1-based cells.
#   - PDF on Kitty/Ghostty:     pixels (SGR 1016 is enabled per PDF session).
#                               cx;cy must be device pixels; the app divides by
#                               the cell size to recover the cell.
# Tapes always use 1-based CELL coordinates; when KITTY_PIXEL_MOUSE=true we
# convert to the pixel center of the target cell using a one-time calibration.

KITTY_PIXEL_MOUSE=false   # set by the runner: true for PDF on Kitty/Ghostty
KITTY_CELL_W=""           # device pixels per cell, computed once per session
KITTY_CELL_H=""

# Compute cell pixel size once: capture the window, read its pixel dims, divide
# by the grid (columns x lines) from `kitty @ ls`. With hide_window_decorations
# and zero padding the capture is exactly grid*cell, so this is exact.
kitty_calibrate_cell_size() {
    [ -n "$KITTY_CELL_W" ] && return 0
    local macos_id="$KITTY_MACOS_WINDOW_ID"
    [ -z "$macos_id" ] && macos_id=$(get_any_kitty_macos_window_id 2>/dev/null)
    if [ -z "$macos_id" ]; then
        echo "WARNING: no macOS window id; cannot calibrate cell size" >&2
        return 1
    fi
    local tmp="${SCRATCHPAD:-/tmp}/kitty_calib_$$.png"
    screencapture -l"$macos_id" -x -o "$tmp" 2>/dev/null
    if [ ! -s "$tmp" ]; then
        echo "WARNING: calibration capture failed" >&2
        return 1
    fi
    local pxw pxh
    pxw=$(sips -g pixelWidth "$tmp" 2>/dev/null | awk '/pixelWidth/{print $2}')
    pxh=$(sips -g pixelHeight "$tmp" 2>/dev/null | awk '/pixelHeight/{print $2}')
    rm -f "$tmp"
    local grid
    grid=$("$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null | WID="$KITTY_WINDOW_ID" python3 -c "
import sys, json, os
wid = int(os.environ['WID'])
d = json.load(sys.stdin)
for ow in d:
    for t in ow.get('tabs', []):
        for w in t.get('windows', []):
            if w.get('id') == wid:
                print(w.get('columns'), w.get('lines')); sys.exit(0)
")
    local cols lines
    read -r cols lines <<< "$grid"
    if [[ ! "$pxw" =~ ^[0-9]+$ || ! "$cols" =~ ^[0-9]+$ || "$cols" -eq 0 || "$lines" -eq 0 ]]; then
        echo "WARNING: calibration failed (px=${pxw}x${pxh} grid=${cols}x${lines})" >&2
        return 1
    fi
    KITTY_CELL_W=$(echo "scale=4; $pxw / $cols" | bc -l)
    KITTY_CELL_H=$(echo "scale=4; $pxh / $lines" | bc -l)
    log_verbose "cell size: ${KITTY_CELL_W}x${KITTY_CELL_H}px (window ${pxw}x${pxh}, grid ${cols}x${lines})"
    return 0
}

# Map an input coordinate to the SGR cx;cy value to send. Echoes "cx cy".
#
# Three modes:
#   - KITTY_MOUSE_RAW_PX=true : inputs are DEVICE PIXELS (the *px tape commands).
#     SGR value = pixel + 1 because the parser subtracts 1 to recover the pixel.
#     Gives full sub-cell precision (?1016). Fractional pixels are rounded.
#   - KITTY_PIXEL_MOUSE=true   : inputs are 1-based CELLS; we return the pixel
#     center of the target cell (requires calibration).
#   - otherwise                : inputs are 1-based CELLS sent as-is (SGR 1006).
_kitty_cell_to_sgr() {
    local col="$1" row="$2"
    if [ "${KITTY_MOUSE_RAW_PX:-false}" = true ]; then
        local cx cy
        cx=$(printf '%.0f' "$(echo "$col + 1" | bc -l)")
        cy=$(printf '%.0f' "$(echo "$row + 1" | bc -l)")
        echo "$cx $cy"
    elif [ "$KITTY_PIXEL_MOUSE" = true ]; then
        kitty_calibrate_cell_size || { echo "$col $row"; return; }
        local cx cy
        cx=$(printf '%.0f' "$(echo "($col - 0.5) * $KITTY_CELL_W + 1" | bc -l)")
        cy=$(printf '%.0f' "$(echo "($row - 0.5) * $KITTY_CELL_H + 1" | bc -l)")
        echo "$cx $cy"
    else
        echo "$col $row"
    fi
}

# Send one raw SGR mouse report: button-code, cx, cy, final char (M=press/motion,
# m=release). Uses a literal ESC byte so kitty's send-text passes it through.
_kitty_mouse_raw() {
    local cb="$1" cx="$2" cy="$3" final="$4"
    [ -z "$KITTY_WINDOW_ID" ] && { echo "WARNING: no window for mouse" >&2; return 1; }
    local esc; esc=$(printf '\033')
    "$KITTY_CMD" @ --to "$KITTY_SOCKET" send-text --match "id:$KITTY_WINDOW_ID" \
        "${esc}[<${cb};${cx};${cy}${final}" 2>/dev/null
}

# Button name -> SGR base code.
_kitty_button_code() {
    case "$1" in
        right)  echo 2 ;;
        middle) echo 1 ;;
        left|*) echo 0 ;;
    esac
}

# Click (press+release) at a cell. Usage: send_kitty_mouse_click COL ROW [button]
send_kitty_mouse_click() {
    local col="$1" row="$2" button="${3:-left}"
    local cb; cb=$(_kitty_button_code "$button")
    local sgr; sgr=$(_kitty_cell_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    _kitty_mouse_raw "$cb" "$cx" "$cy" "M"
    _kitty_mouse_raw "$cb" "$cx" "$cy" "m"
}

# Multi-click at a cell (2=double, 3=triple). The app's MouseTracker groups them
# by time+distance, so we send them back-to-back with no delay.
send_kitty_mouse_multiclick() {
    local count="$1" col="$2" row="$3" button="${4:-left}"
    local cb; cb=$(_kitty_button_code "$button")
    local sgr; sgr=$(_kitty_cell_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    local i
    for ((i = 0; i < count; i++)); do
        _kitty_mouse_raw "$cb" "$cx" "$cy" "M"
        _kitty_mouse_raw "$cb" "$cx" "$cy" "m"
    done
}

# Drag from one cell to another: press, motion (with button-held bit 32),
# release. Sends a midpoint motion too so selection rendering tracks the path.
send_kitty_mouse_drag() {
    local c1="$1" r1="$2" c2="$3" r2="$4" button="${5:-left}"
    local cb; cb=$(_kitty_button_code "$button")
    local motion=$((cb + 32))
    local s1 s2 sm
    s1=$(_kitty_cell_to_sgr "$c1" "$r1")
    s2=$(_kitty_cell_to_sgr "$c2" "$r2")
    local mcol=$(( (c1 + c2) / 2 )) mrow=$(( (r1 + r2) / 2 ))
    sm=$(_kitty_cell_to_sgr "$mcol" "$mrow")
    local x1 y1 x2 y2 xm ym
    read -r x1 y1 <<< "$s1"; read -r x2 y2 <<< "$s2"; read -r xm ym <<< "$sm"
    _kitty_mouse_raw "$cb" "$x1" "$y1" "M"       # press at start
    _kitty_mouse_raw "$motion" "$xm" "$ym" "M"   # drag through midpoint
    _kitty_mouse_raw "$motion" "$x2" "$y2" "M"   # drag to end
    _kitty_mouse_raw "$cb" "$x2" "$y2" "m"       # release at end
}

# Scroll wheel at a cell. Usage: send_kitty_mouse_scroll up|down COL ROW [count]
send_kitty_mouse_scroll() {
    local dir="$1" col="$2" row="$3" count="${4:-1}"
    local cb
    case "$dir" in
        up)   cb=64 ;;
        down) cb=65 ;;
        *) echo "WARNING: scroll dir must be up|down" >&2; return 1 ;;
    esac
    local sgr; sgr=$(_kitty_cell_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    local i
    for ((i = 0; i < count; i++)); do
        _kitty_mouse_raw "$cb" "$cx" "$cy" "M"
    done
}

# Mouse move (no button) to a cell.
send_kitty_mouse_move() {
    local col="$1" row="$2"
    local sgr; sgr=$(_kitty_cell_to_sgr "$col" "$row")
    local cx cy; read -r cx cy <<< "$sgr"
    _kitty_mouse_raw "35" "$cx" "$cy" "M"   # 35 = motion, no button (3 + 32)
}

# Close the bookokrat window and WAIT until it's actually gone, so windows don't
# accumulate across tapes (overlapping windows occlude each other -> blank
# captures). `close-window` reliably closes the (sole) window of its os-window.
close_kitty() {
    local wid="$KITTY_WINDOW_ID"
    if [ -n "$wid" ]; then
        "$KITTY_CMD" @ --to "$KITTY_SOCKET" close-window --match "id:$wid" 2>/dev/null || true
        local i
        for i in $(seq 1 25); do
            "$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null \
                | WID="$wid" python3 -c "import sys,json,os
wid=int(os.environ['WID'])
found=any(w.get('id')==wid for ow in json.load(sys.stdin) for t in ow.get('tabs',[]) for w in t.get('windows',[]))
sys.exit(1 if found else 0)" && break
            sleep 0.1
        done
    fi
    KITTY_WINDOW_ID=""
    KITTY_OS_WINDOW_ID=""
    KITTY_MACOS_WINDOW_ID=""
}

# Cleanup managed Kitty instance (call at end of test session)
cleanup_kitty() {
    if $KITTY_MANAGED; then
        echo "Terminating managed Kitty instance..."
        [ -n "$KITTY_PID" ] && kill -9 "$KITTY_PID" 2>/dev/null || true
        # `open` detaches, so also match by the unique socket path.
        [ -n "$KITTY_SOCKET_BASE" ] && pkill -f "listen_on=unix:$KITTY_SOCKET_BASE" 2>/dev/null || true
        if [ -n "$KITTY_SOCKET" ]; then
            rm -f "${KITTY_SOCKET#unix:}" 2>/dev/null || true
        fi
    fi

    KITTY_PID=""
    KITTY_MANAGED=false
    KITTY_SOCKET=""
    KITTY_SOCKET_BASE=""
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

# Resolve KITTY_MACOS_WINDOW_ID from kitty's own `ls` by our window id. This is
# the reliable path: term_launch runs in a command substitution (subshell), so
# the platform_window_id captured inside launch_kitty is lost in the parent. We
# re-query it here. The platform_window_id IS the macOS CGWindowNumber that
# screencapture -l needs, and works regardless of on-screen/frontmost status —
# unlike the swift on-screen enumeration below, which is flaky for background
# windows and can accidentally match the user's own kitty.
resolve_kitty_macos_window_id() {
    [ -z "$KITTY_WINDOW_ID" ] && return 1
    local pid
    pid=$("$KITTY_CMD" @ --to "$KITTY_SOCKET" ls 2>/dev/null | WID="$KITTY_WINDOW_ID" python3 -c "
import sys, json, os
wid = int(os.environ['WID'])
try:
    d = json.load(sys.stdin)
except Exception:
    raise SystemExit
for ow in d:
    pw = ow.get('platform_window_id')
    for t in ow.get('tabs', []):
        for w in t.get('windows', []):
            if w.get('id') == wid and pw is not None:
                print(pw); raise SystemExit
" 2>/dev/null)
    if [[ "$pid" =~ ^[0-9]+$ ]]; then
        KITTY_MACOS_WINDOW_ID="$pid"
        return 0
    fi
    return 1
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
