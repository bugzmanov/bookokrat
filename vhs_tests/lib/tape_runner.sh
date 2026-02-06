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
            # Kitty: use macOS screencapture with cached window ID
            local macos_id="$KITTY_MACOS_WINDOW_ID"
            # Refresh if not set
            if [ -z "$macos_id" ]; then
                macos_id=$(get_kitty_macos_window_id "$WINDOW_TITLE")
            fi
            if [ -z "$macos_id" ]; then
                macos_id=$(get_any_kitty_macos_window_id)
            fi
            if [ -n "$macos_id" ]; then
                screencapture -l"$macos_id" -x -o "$output_path" 2>/dev/null
            else
                log_error "Could not find Kitty window for screenshot"
            fi
            ;;
        wezterm)
            # WezTerm: use macOS screencapture with window ID
            local macos_id="$WEZTERM_MACOS_WINDOW_ID"
            if [ -z "$macos_id" ]; then
                macos_id=$(get_any_wezterm_macos_window_id)
            fi
            if [ -n "$macos_id" ]; then
                screencapture -l"$macos_id" -x -o "$output_path" 2>/dev/null
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
                ghostty|*) send_ctrl_key_repeated "$rkey" "$rcount" "$rdelay" ;;
            esac
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

        ctrl)
            if [ -z "$arg" ]; then
                log_error "ctrl requires a character"
                return 1
            fi
            log_verbose "ctrl+$arg"
            term_send_ctrl_key "$arg"
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
