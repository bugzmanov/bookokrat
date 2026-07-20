#!/usr/bin/env bash
#
# tape_runner.sh - VHS-style tape parser and executor
#
# Parses .tape files and executes commands sequentially.
# Supports multiple terminals: ghostty, kitty
#
# Requires window_manager.sh or kitty_manager.sh to be sourced first (done by run.sh)

# Global state
TAPE_SCREENSHOTS=()      # Array of screenshot names taken
TAPE_ERRORS=()           # Array of errors encountered
CURRENT_TAPE=""          # Current tape being run
VERBOSE=${VERBOSE:-false}
TERMINAL_TYPE=${TERMINAL_TYPE:-ghostty}  # ghostty or kitty

# Wait time multiplier per terminal (WezTerm is slower)
get_wait_multiplier() {
    case "$TERMINAL_TYPE" in
        wezterm) echo "2.0" ;;
        *)       echo "1.0" ;;
    esac
}

log_verbose() {
    if $VERBOSE; then
        echo "  [TAPE] $*"
    fi
}

log_info() {
    echo "  $*"
}

log_error() {
    echo "  ERROR: $*" >&2
    TAPE_ERRORS+=("$*")
}

# Terminal abstraction functions
# These call the appropriate terminal-specific functions based on TERMINAL_TYPE

term_launch() {
    local title="$1"
    local binary="$2"
    local test_file="$3"

    case "$TERMINAL_TYPE" in
        kitty)
            launch_kitty "$title" "$binary" "$test_file"
            ;;
        wezterm)
            launch_wezterm "$title" "$binary" "$test_file"
            ;;
        ghostty|*)
            launch_ghostty "$title" "$binary" "$test_file"
            ;;
    esac
}

term_capture() {
    local output_path="$1"

    case "$TERMINAL_TYPE" in
        kitty)
            # Kitty: use macOS screencapture with the platform_window_id.
            # Prefer kitty's own `ls` (reliable; the cached global is lost across
            # term_launch's subshell), then fall back to swift enumeration.
            local macos_id="$KITTY_MACOS_WINDOW_ID"
            if [ -z "$macos_id" ]; then
                resolve_kitty_macos_window_id && macos_id="$KITTY_MACOS_WINDOW_ID"
            fi
            if [ -z "$macos_id" ]; then
                macos_id=$(get_kitty_macos_window_id "$WINDOW_TITLE")
            fi
            if [ -z "$macos_id" ]; then
                macos_id=$(get_any_kitty_macos_window_id)
            fi
            if [ -n "$macos_id" ]; then
                # CRITICAL: delete any prior capture first. screencapture of an
                # occluded / off-active-Space window silently writes nothing; if a
                # stale file from an earlier run remained, the harness would treat
                # it as a fresh success and compare stale pixels. Removing it makes
                # a failed capture leave no file -> honest "Failed to capture".
                rm -f "$output_path"
                screencapture -l"$macos_id" -x -o "$output_path" 2>/dev/null
                # macOS won't capture a never-foregrounded / occluded window. If
                # the first (no-focus) capture produced nothing, bring the test OS
                # window to the front (activate kitty + focus the os-window) and
                # retry. Steals focus only when needed.
                if [ ! -s "$output_path" ]; then
                    # Foreground the SPECIFIC test kitty process (by pid) onto the
                    # active Space — `activate "kitty"` is ambiguous with the
                    # user's own kitty instances and can raise the wrong window /
                    # wrong Space. System Events targeting the test pid is exact.
                    if [ -n "$KITTY_PID" ]; then
                        osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $KITTY_PID) to true" 2>/dev/null
                    fi
                    "$KITTY_CMD" @ --to "$KITTY_SOCKET" focus-os-window \
                        --match "id:$KITTY_WINDOW_ID" 2>/dev/null
                    sleep 0.5
                    rm -f "$output_path"
                    screencapture -l"$macos_id" -x -o "$output_path" 2>/dev/null
                fi
            else
                log_error "Could not find Kitty window for screenshot"
            fi
            ;;
        wezterm)
            # WezTerm: use macOS screencapture with window ID. Same rules as the
            # kitty branch: delete stale output first (an occluded window's
            # capture silently writes nothing and a stale file would fake a
            # success), and if the no-focus capture is empty, foreground the
            # test WezTerm and retry — screencapture cannot image a window
            # that is on another Space / never foregrounded.
            local macos_id="$WEZTERM_MACOS_WINDOW_ID"
            if [ -z "$macos_id" ]; then
                macos_id=$(get_any_wezterm_macos_window_id)
            fi
            if [ -n "$macos_id" ]; then
                rm -f "$output_path"
                screencapture -l"$macos_id" -x -o "$output_path" 2>/dev/null
                if [ ! -s "$output_path" ]; then
                    osascript -e 'tell application "WezTerm" to activate' 2>/dev/null
                    sleep 0.5
                    # The id may have been stale (window re-created); re-resolve.
                    macos_id=$(get_any_wezterm_macos_window_id)
                    [ -z "$macos_id" ] && macos_id="$WEZTERM_MACOS_WINDOW_ID"
                    rm -f "$output_path"
                    screencapture -l"$macos_id" -x -o "$output_path"
                fi
            else
                log_error "Could not find WezTerm window for screenshot"
            fi
            ;;
        ghostty|*)
            capture_screenshot "$GHOSTTY_WINDOW_ID" "$output_path"
            ;;
    esac
}

term_send_key() {
    local key="$1"

    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_key "$key"
            ;;
        wezterm)
            send_wezterm_key "$key"
            ;;
        ghostty|*)
            send_key "$key"
            ;;
    esac
}

term_send_ctrl_key() {
    local key="$1"

    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_ctrl_key "$key"
            ;;
        wezterm)
            send_wezterm_ctrl_key "$key"
            ;;
        ghostty|*)
            send_ctrl_key "$key"
            ;;
    esac
}

term_send_ctrl_shift_key() {
    local key="$1"

    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_ctrl_shift_key "$key"
            ;;
        wezterm)
            send_wezterm_ctrl_shift_key "$key"
            ;;
        ghostty|*)
            send_ctrl_shift_key "$key"
            ;;
    esac
}

term_send_escape() {
    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_escape
            ;;
        wezterm)
            send_wezterm_escape
            ;;
        ghostty|*)
            send_escape
            ;;
    esac
}

term_send_return() {
    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_return
            ;;
        wezterm)
            send_wezterm_return
            ;;
        ghostty|*)
            send_return
            ;;
    esac
}

term_send_tab() {
    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_key "tab"
            ;;
        wezterm)
            send_wezterm_tab
            ;;
        ghostty|*)
            send_tab
            ;;
    esac
}

term_send_shift_tab() {
    case "$TERMINAL_TYPE" in
        kitty)
            send_kitty_key "shift+tab"
            ;;
        wezterm)
            send_wezterm_shift_tab
            ;;
        ghostty|*)
            send_shift_tab
            ;;
    esac
}

# Mouse dispatch. Only Kitty is implemented (the background test terminal);
# other terminals log a clear error so tapes fail loudly instead of silently.
term_mouse_click() {        # COL ROW [button]
    case "$TERMINAL_TYPE" in
        kitty) send_kitty_mouse_click "$@" ;;
        wezterm) send_wezterm_mouse_click "$@" ;;
        *) log_error "mouse not supported for $TERMINAL_TYPE" ;;
    esac
}
term_mouse_multiclick() {   # COUNT COL ROW [button]
    case "$TERMINAL_TYPE" in
        kitty) send_kitty_mouse_multiclick "$@" ;;
        wezterm) send_wezterm_mouse_multiclick "$@" ;;
        *) log_error "mouse not supported for $TERMINAL_TYPE" ;;
    esac
}
term_mouse_drag() {         # C1 R1 C2 R2 [button]
    case "$TERMINAL_TYPE" in
        kitty) send_kitty_mouse_drag "$@" ;;
        wezterm) send_wezterm_mouse_drag "$@" ;;
        *) log_error "mouse not supported for $TERMINAL_TYPE" ;;
    esac
}
term_mouse_scroll() {       # up|down COL ROW [count]
    case "$TERMINAL_TYPE" in
        kitty) send_kitty_mouse_scroll "$@" ;;
        wezterm) send_wezterm_mouse_scroll "$@" ;;
        *) log_error "mouse not supported for $TERMINAL_TYPE" ;;
    esac
}
term_mouse_move() {         # COL ROW
    case "$TERMINAL_TYPE" in
        kitty) send_kitty_mouse_move "$@" ;;
        wezterm) send_wezterm_mouse_move "$@" ;;
        *) log_error "mouse not supported for $TERMINAL_TYPE" ;;
    esac
}

term_close() {
    case "$TERMINAL_TYPE" in
        kitty)
            close_kitty
            ;;
        wezterm)
            close_wezterm
            ;;
        ghostty|*)
            close_ghostty
            ;;
    esac
}

# Global: file override from tape (EPUB/PDF)
TAPE_FILE=""
TAPE_PDF_FILE=""
TAPE_NOFILE=false         # If true, launch without a file argument
TAPE_WINDOW_PERCENT=""    # Window size as percent of screen (empty = maximize)
TAPE_PENDING_DESC=""      # Caption for the NEXT screenshot (set by `desc`)

# Parse and execute a single tape command
# Returns 0 on success, 1 on error
execute_command() {
    local cmd="$1"
    local arg="$2"

    case "$cmd" in
        pdf)
            # Set the PDF file to use (resolved relative to project root)
            if [ -z "$arg" ]; then
                log_error "pdf requires a file path"
                return 1
            fi
            TAPE_PDF_FILE="$arg"
            log_verbose "PDF file: $arg"
            return 0
            ;;
        file)
            # Set any file to use (EPUB/PDF)
            if [ -z "$arg" ]; then
                log_error "file requires a file path"
                return 1
            fi
            TAPE_FILE="$arg"
            log_verbose "File: $arg"
            return 0
            ;;
        nofile)
            # Launch without opening a specific file (shows book list)
            TAPE_NOFILE=true
            log_verbose "No file mode"
            return 0
            ;;
        window)
            # Window size directive (handled before launch, skip during execution)
            return 0
            ;;
        appenv)
            # App env directive (handled before launch, skip during execution)
            return 0
            ;;
        terminal)
            # Terminal restriction directive (handled by run.sh tape selection)
            return 0
            ;;
        type)
            # Type a string with visible per-character delay (single osascript call)
            if [ -z "$arg" ]; then
                log_error "type requires a string"
                return 1
            fi
            log_verbose "type: $arg"
            case "$TERMINAL_TYPE" in
                ghostty|*) send_type_slow "$arg" 0.03 ;;
            esac
            ;;
        rapid)
            # Type keys instantly (no per-character delay, for zoom/margin keys)
            if [ -z "$arg" ]; then
                log_error "rapid requires a string"
                return 1
            fi
            log_verbose "rapid: $arg"
            case "$TERMINAL_TYPE" in
                ghostty|*) send_type "$arg" ;;
            esac
            ;;
        repeat_key)
            # Send a key N times in a single osascript (avoids per-command overhead)
            # Usage: repeat_key <key> <count> [delay_seconds]
            local rkey=$(echo "$arg" | awk '{print $1}')
            local rcount=$(echo "$arg" | awk '{print $2}')
            local rdelay=$(echo "$arg" | awk '{print $3}')
            rdelay="${rdelay:-0.05}"
            if [ -z "$rkey" ] || [ -z "$rcount" ]; then
                log_error "repeat_key requires: key count [delay]"
                return 1
            fi
            log_verbose "repeat_key: $rkey x$rcount (delay: ${rdelay}s)"
            case "$TERMINAL_TYPE" in
                kitty|wezterm)
                    # kitty/wezterm have no osascript batch sender - loop the
                    # socket/pty send.
                    local ri=0
                    while [ "$ri" -lt "$rcount" ]; do
                        term_send_key "$rkey"
                        sleep "$rdelay"
                        ri=$((ri + 1))
                    done
                    ;;
                ghostty|*) send_key_repeated "$rkey" "$rcount" "$rdelay" ;;
            esac
            ;;
        repeat_ctrl)
            # Send ctrl+key N times in a single osascript (avoids per-command overhead)
            # Usage: repeat_ctrl <key> <count> [delay_seconds]
            local rkey=$(echo "$arg" | awk '{print $1}')
            local rcount=$(echo "$arg" | awk '{print $2}')
            local rdelay=$(echo "$arg" | awk '{print $3}')
            rdelay="${rdelay:-0.1}"
            if [ -z "$rkey" ] || [ -z "$rcount" ]; then
                log_error "repeat_ctrl requires: key count [delay]"
                return 1
            fi
            log_verbose "repeat_ctrl: $rkey x$rcount (delay: ${rdelay}s)"
            case "$TERMINAL_TYPE" in
                kitty|wezterm)
                    local ci=0
                    while [ "$ci" -lt "$rcount" ]; do
                        term_send_ctrl_key "$rkey"
                        sleep "$rdelay"
                        ci=$((ci + 1))
                    done
                    ;;
                ghostty|*) send_ctrl_key_repeated "$rkey" "$rcount" "$rdelay" ;;
            esac
            ;;
        about)
            # Tape-level description, shown in the report header.
            if [ -n "$arg" ]; then
                printf '%s\n' "$arg" > "$OUTPUT_DIR/_about.txt"
                log_verbose "about: $arg"
            fi
            ;;

        desc)
            # Caption for the NEXT screenshot: what is done + what to expect.
            TAPE_PENDING_DESC="$arg"
            log_verbose "desc: $arg"
            ;;

        screenshot)
            if [ -z "$arg" ]; then
                log_error "screenshot requires a name"
                return 1
            fi
            local output_path="$OUTPUT_DIR/$arg.png"
            log_info "📸 screenshot: $arg"
            term_capture "$output_path"
            if [ $? -eq 0 ] && [ -f "$output_path" ]; then
                TAPE_SCREENSHOTS+=("$arg")
                # Persist the pending caption as a sidecar so the report (a
                # separate process) can show it next to the snapshot.
                if [ -n "$TAPE_PENDING_DESC" ]; then
                    printf '%s\n' "$TAPE_PENDING_DESC" > "$OUTPUT_DIR/$arg.desc.txt"
                fi
                TAPE_PENDING_DESC=""
                log_verbose "Saved to $output_path"
            else
                log_error "Failed to capture screenshot: $arg"
                return 1
            fi
            ;;

        key)
            if [ -z "$arg" ]; then
                log_error "key requires a character"
                return 1
            fi
            log_verbose "key: $arg"
            term_send_key "$arg"
            ;;

        shell)
            # Run a host shell command from the project root, mid-tape. Used by
            # tapes that need to mutate state outside the app: overwrite the
            # opened PDF (file-watch reload test), poke the synctex editor
            # socket, etc. The command runs synchronously; the tape continues
            # even if it fails (the failure is logged and the screenshots will
            # show the missing effect).
            if [ -z "$arg" ]; then
                log_error "shell requires a command"
                return 1
            fi
            log_verbose "shell: $arg"
            (cd "$PROJECT_ROOT" && bash -c "$arg") || log_error "shell command failed: $arg"
            ;;

        ctrl)
            if [ -z "$arg" ]; then
                log_error "ctrl requires a character"
                return 1
            fi
            log_verbose "ctrl+$arg"
            term_send_ctrl_key "$arg"
            ;;

        ctrl_shift)
            if [ -z "$arg" ]; then
                log_error "ctrl_shift requires a character"
                return 1
            fi
            log_verbose "ctrl+shift+$arg"
            term_send_ctrl_shift_key "$arg"
            ;;

        escape)
            log_verbose "escape"
            term_send_escape
            ;;

        return)
            log_verbose "return"
            term_send_return
            ;;

        tab)
            log_verbose "tab"
            term_send_tab
            ;;

        shift_tab)
            log_verbose "shift+tab"
            term_send_shift_tab
            ;;

        click|rclick|mclick|clickpx|rclickpx|mclickpx)
            # CELL: click COL ROW [button]   PIXEL: clickpx X Y [button]
            # Coords are 1-based cells by default; the *px variants take device
            # pixels (sub-cell precision via ?1016 on Kitty/Ghostty PDF).
            local base="${cmd%px}"
            [[ "$cmd" == *px ]] && KITTY_MOUSE_RAW_PX=true
            local mcol=$(echo "$arg" | awk '{print $1}')
            local mrow=$(echo "$arg" | awk '{print $2}')
            local mbtn=$(echo "$arg" | awk '{print $3}')
            [ "$base" = "rclick" ] && mbtn="right"
            [ "$base" = "mclick" ] && mbtn="middle"
            mbtn="${mbtn:-left}"
            if [ -z "$mcol" ] || [ -z "$mrow" ]; then
                KITTY_MOUSE_RAW_PX=false; log_error "$cmd requires: X Y [button]"; return 1
            fi
            log_verbose "$cmd: ($mcol,$mrow) $mbtn"
            term_mouse_click "$mcol" "$mrow" "$mbtn"
            KITTY_MOUSE_RAW_PX=false
            ;;

        doubleclick|tripleclick|doubleclickpx|tripleclickpx)
            [[ "$cmd" == *px ]] && KITTY_MOUSE_RAW_PX=true
            local base="${cmd%px}"
            local mcol=$(echo "$arg" | awk '{print $1}')
            local mrow=$(echo "$arg" | awk '{print $2}')
            local mbtn=$(echo "$arg" | awk '{print $3}')
            mbtn="${mbtn:-left}"
            if [ -z "$mcol" ] || [ -z "$mrow" ]; then
                KITTY_MOUSE_RAW_PX=false; log_error "$cmd requires: X Y [button]"; return 1
            fi
            local mcount=2; [ "$base" = "tripleclick" ] && mcount=3
            log_verbose "$cmd: ($mcol,$mrow) $mbtn"
            term_mouse_multiclick "$mcount" "$mcol" "$mrow" "$mbtn"
            KITTY_MOUSE_RAW_PX=false
            ;;

        drag|dragpx)
            # CELL: drag C1 R1 C2 R2 [button]   PIXEL: dragpx X1 Y1 X2 Y2 [button]
            [[ "$cmd" == *px ]] && KITTY_MOUSE_RAW_PX=true
            local d1=$(echo "$arg" | awk '{print $1}')
            local d2=$(echo "$arg" | awk '{print $2}')
            local d3=$(echo "$arg" | awk '{print $3}')
            local d4=$(echo "$arg" | awk '{print $4}')
            local dbtn=$(echo "$arg" | awk '{print $5}')
            dbtn="${dbtn:-left}"
            if [ -z "$d1" ] || [ -z "$d2" ] || [ -z "$d3" ] || [ -z "$d4" ]; then
                KITTY_MOUSE_RAW_PX=false; log_error "$cmd requires: X1 Y1 X2 Y2 [button]"; return 1
            fi
            log_verbose "$cmd: ($d1,$d2) -> ($d3,$d4) $dbtn"
            term_mouse_drag "$d1" "$d2" "$d3" "$d4" "$dbtn"
            KITTY_MOUSE_RAW_PX=false
            ;;

        scroll|scrollpx)
            # scroll up|down COL ROW [count]  (px: scrollpx up|down X Y [count])
            [[ "$cmd" == *px ]] && KITTY_MOUSE_RAW_PX=true
            local sdir=$(echo "$arg" | awk '{print $1}')
            local scol=$(echo "$arg" | awk '{print $2}')
            local srow=$(echo "$arg" | awk '{print $3}')
            local scnt=$(echo "$arg" | awk '{print $4}')
            scnt="${scnt:-1}"
            if [ -z "$sdir" ] || [ -z "$scol" ] || [ -z "$srow" ]; then
                KITTY_MOUSE_RAW_PX=false; log_error "$cmd requires: up|down X Y [count]"; return 1
            fi
            log_verbose "$cmd: $sdir at ($scol,$srow) x$scnt"
            term_mouse_scroll "$sdir" "$scol" "$srow" "$scnt"
            KITTY_MOUSE_RAW_PX=false
            ;;

        mousemove|mousemovepx)
            [[ "$cmd" == *px ]] && KITTY_MOUSE_RAW_PX=true
            local mcol=$(echo "$arg" | awk '{print $1}')
            local mrow=$(echo "$arg" | awk '{print $2}')
            if [ -z "$mcol" ] || [ -z "$mrow" ]; then
                KITTY_MOUSE_RAW_PX=false; log_error "$cmd requires: X Y"; return 1
            fi
            log_verbose "$cmd: ($mcol,$mrow)"
            term_mouse_move "$mcol" "$mrow"
            KITTY_MOUSE_RAW_PX=false
            ;;

        wait)
            local ms="${arg:-500}"
            # Apply terminal-specific multiplier
            local multiplier=$(get_wait_multiplier)
            local adjusted_ms=$(echo "scale=0; $ms * $multiplier / 1" | bc)
            log_verbose "wait: ${ms}ms (adjusted: ${adjusted_ms}ms, multiplier: ${multiplier}x)"
            # Convert ms to seconds with decimal
            local secs=$(echo "scale=3; $adjusted_ms / 1000" | bc)
            sleep "$secs"
            ;;

        *)
            log_error "Unknown command: $cmd"
            return 1
            ;;
    esac

    return 0
}

# Parse a tape file and return commands
# Each line is: "command arg" or just "command"
parse_tape() {
    local tape_file="$1"

    if [ ! -f "$tape_file" ]; then
        echo "ERROR: Tape file not found: $tape_file" >&2
        return 1
    fi

    while IFS= read -r line || [ -n "$line" ]; do
        # Skip empty lines and comments
        line=$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        if [ -z "$line" ] || [[ "$line" == \#* ]]; then
            continue
        fi

        # Per-terminal conditional lines: "@kitty <command>" runs the command
        # only when TERMINAL_TYPE matches (used for terminal-specific
        # coordinates, e.g. clickpx on kitty vs wezterm geometry).
        if [[ "$line" == @* ]]; then
            local cond_term="${line%% *}"
            cond_term="${cond_term#@}"
            if [ "$cond_term" != "$TERMINAL_TYPE" ]; then
                continue
            fi
            line=$(echo "$line" | awk '{$1=""; print $0}' | sed 's/^[[:space:]]*//')
            [ -z "$line" ] && continue
        fi

        # Extract command and argument
        local cmd=$(echo "$line" | awk '{print $1}')
        local arg=$(echo "$line" | awk '{$1=""; print $0}' | sed 's/^[[:space:]]*//')

        echo "$cmd|$arg"
    done < "$tape_file"
}

# Run a tape file
# Usage: run_tape TAPE_FILE BINARY_PATH DEFAULT_TEST_FILE OUTPUT_DIR
# Sets: TAPE_SCREENSHOTS array with names of screenshots taken
run_tape() {
    local tape_file="$1"
    local binary="$2"
    local default_test_file="$3"
    OUTPUT_DIR="$4"

    TAPE_SCREENSHOTS=()
    TAPE_ERRORS=()
    TAPE_FILE=""
    TAPE_PDF_FILE=""
    TAPE_NOFILE=false
    TAPE_WINDOW_PERCENT=""
    TAPE_PENDING_DESC=""
    CURRENT_TAPE=$(basename "$tape_file" .tape)

    local tape_name=$(basename "$tape_file")
    local terminal_label=$(echo "$TERMINAL_TYPE" | tr '[:lower:]' '[:upper:]')

    local wait_mult=$(get_wait_multiplier)

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Running tape: $tape_name [$terminal_label]"
    if [ "$wait_mult" != "1.0" ]; then
        echo "  Wait multiplier: ${wait_mult}x"
    fi
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Parse tape first to validate and extract pdf directive
    local commands=$(parse_tape "$tape_file")
    if [ $? -ne 0 ]; then
        log_error "Failed to parse tape"
        return 1
    fi

    # Extract file directive if present (before other commands)
    local file_line=$(echo "$commands" | grep "^file|" | head -1)
    if [ -n "$file_line" ]; then
        TAPE_FILE=$(echo "$file_line" | cut -d'|' -f2)
        log_info "File from tape: $TAPE_FILE"
    fi

    # Extract pdf directive if present (before other commands)
    local pdf_line=$(echo "$commands" | grep "^pdf|" | head -1)
    if [ -n "$pdf_line" ]; then
        TAPE_PDF_FILE=$(echo "$pdf_line" | cut -d'|' -f2)
        log_info "PDF from tape: $TAPE_PDF_FILE"
    fi

    # Extract nofile directive
    if echo "$commands" | grep -q "^nofile|"; then
        TAPE_NOFILE=true
        log_info "No-file mode (book list)"
    fi

    # Extract window size directive
    local window_line=$(echo "$commands" | grep "^window|" | head -1)
    if [ -n "$window_line" ]; then
        TAPE_WINDOW_PERCENT=$(echo "$window_line" | cut -d'|' -f2 | tr -d ' ')
        log_info "Window size: ${TAPE_WINDOW_PERCENT}%"
    fi

    # Extract appenv directives (NAME=VALUE pairs passed to the app's env)
    TAPE_APP_ENV=""
    local appenv_line
    while IFS= read -r appenv_line; do
        [ -z "$appenv_line" ] && continue
        TAPE_APP_ENV="$TAPE_APP_ENV $(echo "$appenv_line" | cut -d'|' -f2)"
    done < <(echo "$commands" | grep "^appenv|")
    if [ -n "$TAPE_APP_ENV" ]; then
        log_info "App env:$TAPE_APP_ENV"
    fi

    # Use tape's file override, else PDF, else default
    local test_file="${TAPE_FILE:-${TAPE_PDF_FILE:-$default_test_file}}"
    # Resolve relative to project root
    if [[ ! "$test_file" = /* ]]; then
        test_file="$PROJECT_ROOT/$test_file"
    fi

    local cmd_count=$(echo "$commands" | grep -c '|' || echo "0")
    log_info "Parsed $cmd_count commands"

    # Launch the app with --test-mode for reproducible state (no bookmarks/settings)
    log_info "🚀 Launching $TERMINAL_TYPE..."
    WINDOW_TITLE="VHS_TEST_${CURRENT_TAPE}"

    # Configure window size (non-maximize needs resize after launch)
    if [ -n "$TAPE_WINDOW_PERCENT" ]; then
        GHOSTTY_MAXIMIZE=false
    else
        GHOSTTY_MAXIMIZE=true
    fi

    # Set working directory for nofile mode (app scans cwd for books)
    if [ "$TAPE_NOFILE" = "true" ]; then
        GHOSTTY_WORKING_DIR="$PROJECT_ROOT"
    else
        GHOSTTY_WORKING_DIR=""
    fi

    # Build launch arguments
    local quoted_args=""
    if [ "$TAPE_NOFILE" = "true" ]; then
        quoted_args=" --test-mode"
    else
        for arg in "$test_file" "--test-mode"; do
            quoted_args+=" $(printf %q "$arg")"
        done
    fi
    local window_id=$(term_launch "$WINDOW_TITLE" "$binary" "$quoted_args")

    if [ -z "$window_id" ]; then
        log_error "Failed to launch $TERMINAL_TYPE"
        return 1
    fi

    # Set globals for use by execute_command (subshell loses the assignment)
    case "$TERMINAL_TYPE" in
        kitty)
            KITTY_WINDOW_ID="$window_id"
            # PDF on Kitty enables SGR-pixel mouse (?1016), so mouse commands
            # must send pixel coords. EPUB/book-list use cell coords. Reset the
            # cached cell size so each launch recalibrates.
            KITTY_CELL_W=""; KITTY_CELL_H=""
            if [[ "$test_file" == *.pdf || "$test_file" == *.PDF ]]; then
                KITTY_PIXEL_MOUSE=true
            else
                KITTY_PIXEL_MOUSE=false
            fi
            ;;
        wezterm)
            WEZTERM_PANE_ID="$window_id"
            ;;
        ghostty|*)
            GHOSTTY_WINDOW_ID="$window_id"
            GHOSTTY_WINDOW_TITLE="$WINDOW_TITLE"
            ;;
    esac

    log_info "Window ID: $window_id"

    # Small delay for app to fully render
    sleep 1

    # Resize window if a specific size was requested
    if [ -n "$TAPE_WINDOW_PERCENT" ] && [ "$TERMINAL_TYPE" = "ghostty" ]; then
        log_info "Resizing window to ${TAPE_WINDOW_PERCENT}%"
        resize_ghostty_window "$TAPE_WINDOW_PERCENT"
        sleep 0.5
    fi

    # Execute each command
    # Use while loop with redirect to avoid subshell (preserves array modifications)
    local line_num=0
    while IFS='|' read -r cmd arg; do
        line_num=$((line_num + 1))
        if ! execute_command "$cmd" "$arg"; then
            log_error "Command failed at line $line_num: $cmd $arg"
            # Continue executing remaining commands
        fi
        # Small delay between commands for stability
        sleep 0.1
    done <<< "$commands"

    # Close the app
    log_info "🛑 Closing $TERMINAL_TYPE..."
    term_close

    # Report
    local screenshot_count=${#TAPE_SCREENSHOTS[@]}
    local error_count=${#TAPE_ERRORS[@]}

    echo ""
    echo "  Screenshots: $screenshot_count"
    if [ $error_count -gt 0 ]; then
        echo "  Errors: $error_count"
        return 1
    fi

    return 0
}

# Get list of screenshots taken (for use after run_tape)
get_tape_screenshots() {
    echo "${TAPE_SCREENSHOTS[@]}"
}
