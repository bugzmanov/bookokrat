#!/usr/bin/env bash
#
# report_generator.sh - HTML report generation for VHS tests
#
# Generates a self-contained HTML report with embedded images.
# Requires image_compare.sh to be sourced first (done by run.sh)

# Generate HTML report
# Usage: generate_report TAPE_NAME GOLDEN_DIR ACTUAL_DIR REPORT_PATH SCREENSHOT_NAMES...
generate_report() {
    local tape_name="$1"
    local golden_dir="$2"
    local actual_dir="$3"
    local report_path="$4"
    shift 4
    local screenshots=("$@")

    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local passed=0
    local failed=0
    local missing=0

    # Start HTML
    cat > "$report_path" << 'HEADER'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>VHS Terminal Test Report</title>
    <style>
        :root {
            --bg: #1a1a2e;
            --bg-card: #16213e;
            --bg-hover: #1f2b47;
            --text: #eee;
            --text-dim: #888;
            --pass: #4ade80;
            --fail: #f87171;
            --warn: #fbbf24;
            --border: #334155;
        }
        * { box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, monospace;
            background: var(--bg);
            color: var(--text);
            margin: 0;
            padding: 20px;
            line-height: 1.5;
        }
        .header {
            text-align: center;
            margin-bottom: 30px;
            padding: 20px;
            background: var(--bg-card);
            border-radius: 8px;
            border: 1px solid var(--border);
        }
        .header h1 {
            margin: 0 0 10px 0;
            font-size: 24px;
        }
        .header .meta {
            color: var(--text-dim);
            font-size: 14px;
        }
        .summary {
            display: flex;
            justify-content: center;
            gap: 30px;
            margin: 20px 0;
        }
        .summary-item {
            text-align: center;
        }
        .summary-item .count {
            font-size: 32px;
            font-weight: bold;
        }
        .summary-item .label {
            font-size: 12px;
            color: var(--text-dim);
            text-transform: uppercase;
        }
        .summary-item.pass .count { color: var(--pass); }
        .summary-item.fail .count { color: var(--fail); }
        .summary-item.missing .count { color: var(--warn); }

        .comparison {
            background: var(--bg-card);
            border: 1px solid var(--border);
            border-radius: 8px;
            margin-bottom: 20px;
            overflow: hidden;
        }
        .comparison.pass { border-left: 4px solid var(--pass); }
        .comparison.fail { border-left: 4px solid var(--fail); }
        .comparison.missing { border-left: 4px solid var(--warn); }

        .comparison-header {
            padding: 15px 20px;
            border-bottom: 1px solid var(--border);
            display: flex;
            align-items: center;
            gap: 15px;
        }
        .comparison-header .status {
            font-size: 20px;
        }
        .comparison-header .name {
            font-weight: bold;
            font-size: 16px;
        }
        .comparison-header .message {
            color: var(--text-dim);
            font-size: 14px;
            margin-left: auto;
        }

        .comparison-images {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 20px;
            padding: 20px;
        }
        .comparison-images.has-diff {
            grid-template-columns: 1fr 1fr 1fr;
        }
        .image-box {
            text-align: center;
        }
        .image-box .label {
            font-size: 12px;
            color: var(--text-dim);
            text-transform: uppercase;
            margin-bottom: 10px;
        }
        .image-box img {
            max-width: 100%;
            height: auto;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: #000;
        }
        .image-box .placeholder {
            padding: 40px;
            background: var(--bg);
            border: 1px dashed var(--border);
            border-radius: 4px;
            color: var(--text-dim);
        }

        .comparison-details {
            padding: 15px 20px;
            border-top: 1px solid var(--border);
            font-size: 13px;
            color: var(--text-dim);
            display: flex;
            gap: 30px;
        }

        .update-cmd {
            padding: 15px 20px;
            border-top: 1px solid var(--border);
            background: var(--bg);
        }
        .update-cmd code {
            display: block;
            background: #000;
            padding: 10px;
            border-radius: 4px;
            font-size: 13px;
            overflow-x: auto;
        }
        .copy-btn {
            margin-top: 10px;
            padding: 8px 16px;
            background: var(--bg-card);
            border: 1px solid var(--border);
            border-radius: 4px;
            color: var(--text);
            cursor: pointer;
            font-size: 13px;
        }
        .copy-btn:hover {
            background: var(--bg-hover);
        }

        .footer {
            text-align: center;
            padding: 20px;
            color: var(--text-dim);
            font-size: 12px;
        }
    </style>
</head>
<body>
HEADER

    # Header with tape name and timestamp
    cat >> "$report_path" << EOF
    <div class="header">
        <h1>🎬 VHS Terminal Test Report</h1>
        <div class="meta">
            <strong>Tape:</strong> ${tape_name}.tape &nbsp;|&nbsp;
            <strong>Generated:</strong> ${timestamp}
        </div>
EOF

    # Process each screenshot and count results
    local comparisons=""
    for name in "${screenshots[@]}"; do
        local golden="$golden_dir/$name.png"
        local actual="$actual_dir/$name.png"

        # Run comparison and capture output
        local comp_output=$(compare_images "$golden" "$actual" "$name")
        local status=$(echo "$comp_output" | grep "^STATUS=" | cut -d= -f2)
        local message=$(echo "$comp_output" | grep "^MESSAGE=" | cut -d= -f2-)
        local golden_dims=$(echo "$comp_output" | grep "^GOLDEN_DIMS=" | cut -d= -f2)
        local actual_dims=$(echo "$comp_output" | grep "^ACTUAL_DIMS=" | cut -d= -f2)
        local golden_size=$(echo "$comp_output" | grep "^GOLDEN_SIZE=" | cut -d= -f2)
        local actual_size=$(echo "$comp_output" | grep "^ACTUAL_SIZE=" | cut -d= -f2)

        case "$status" in
            match) passed=$((passed + 1)); status_icon="✅"; status_class="pass" ;;
            mismatch) failed=$((failed + 1)); status_icon="❌"; status_class="fail" ;;
            missing) missing=$((missing + 1)); status_icon="⚠️"; status_class="missing" ;;
            *) failed=$((failed + 1)); status_icon="❌"; status_class="fail" ;;
        esac

        # Get base64 images
        local golden_b64=$(image_to_base64 "$golden")
        local actual_b64=$(image_to_base64 "$actual")

        # Generate diff image for any test with differences
        local diff_b64=""
        local has_diff="false"
        local diff_pct=$(echo "$comp_output" | grep "^DIFF_PCT=" | cut -d= -f2)
        local ssim=$(echo "$comp_output" | grep "^SSIM=" | cut -d= -f2)

        # Generate diff if there's any measurable difference
        if [ -f "$golden" ] && [ -f "$actual" ]; then
            local should_diff="false"
            # Check if there's a non-perfect SSIM
            if [ -n "$ssim" ] && (( $(echo "$ssim < 1.0" | bc -l) )); then
                should_diff="true"
            fi
            # Check if there's a non-zero diff percentage
            if [ -n "$diff_pct" ] && (( $(echo "$diff_pct > 0" | bc -l) )); then
                should_diff="true"
            fi

            if [ "$should_diff" = "true" ]; then
                local diff_path="$actual_dir/${name}_diff.png"
                if generate_diff_image "$golden" "$actual" "$diff_path"; then
                    diff_b64=$(image_to_base64 "$diff_path")
                    has_diff="true"
                fi
            fi
        fi

        # Build comparison HTML
        comparisons+="<div class=\"comparison $status_class\">"
        comparisons+="<div class=\"comparison-header\">"
        comparisons+="<span class=\"status\">$status_icon</span>"
        comparisons+="<span class=\"name\">$name</span>"
        comparisons+="<span class=\"message\">$message</span>"
        comparisons+="</div>"

        if [ "$has_diff" = "true" ]; then
            comparisons+="<div class=\"comparison-images has-diff\">"
        else
            comparisons+="<div class=\"comparison-images\">"
        fi

        # Golden image
        comparisons+="<div class=\"image-box\">"
        comparisons+="<div class=\"label\">Expected (Golden)</div>"
        if [ -n "$golden_b64" ]; then
            comparisons+="<img src=\"$golden_b64\" alt=\"Golden: $name\">"
        else
            comparisons+="<div class=\"placeholder\">No golden snapshot</div>"
        fi
        comparisons+="</div>"

        # Actual image (with diff overlay for failures)
        comparisons+="<div class=\"image-box\">"
        if [ "$has_diff" = "true" ]; then
            comparisons+="<div class=\"label\">Actual (with diff)</div>"
            comparisons+="<img src=\"$diff_b64\" alt=\"Diff: $name\">"
        else
            comparisons+="<div class=\"label\">Actual</div>"
            if [ -n "$actual_b64" ]; then
                comparisons+="<img src=\"$actual_b64\" alt=\"Actual: $name\">"
            else
                comparisons+="<div class=\"placeholder\">No screenshot captured</div>"
            fi
        fi
        comparisons+="</div>"

        # Third column: clean actual for comparison (only when diff exists)
        if [ "$has_diff" = "true" ]; then
            comparisons+="<div class=\"image-box\">"
            comparisons+="<div class=\"label\">Actual (clean)</div>"
            comparisons+="<img src=\"$actual_b64\" alt=\"Actual: $name\">"
            comparisons+="</div>"
        fi

        comparisons+="</div>"

        # Details
        if [ -n "$golden_dims" ] || [ -n "$actual_dims" ]; then
            comparisons+="<div class=\"comparison-details\">"
            [ -n "$golden_dims" ] && comparisons+="<span>Golden: $golden_dims (${golden_size:-?} bytes)</span>"
            [ -n "$actual_dims" ] && comparisons+="<span>Actual: $actual_dims (${actual_size:-?} bytes)</span>"
            comparisons+="</div>"
        fi

        # Update command for failures/missing
        if [ "$status_class" != "pass" ]; then
            local update_cmd="./vhs_tests/run.sh --tape $tape_name --update"
            comparisons+="<div class=\"update-cmd\">"
            comparisons+="<code>$update_cmd</code>"
            comparisons+="<button class=\"copy-btn\" onclick=\"navigator.clipboard.writeText('$update_cmd')\">📋 Copy Update Command</button>"
            comparisons+="</div>"
        fi

        comparisons+="</div>"
    done

    # Write summary
    local total=${#screenshots[@]}
    cat >> "$report_path" << EOF
        <div class="summary">
            <div class="summary-item pass">
                <div class="count">$passed</div>
                <div class="label">Passed</div>
            </div>
            <div class="summary-item fail">
                <div class="count">$failed</div>
                <div class="label">Failed</div>
            </div>
            <div class="summary-item missing">
                <div class="count">$missing</div>
                <div class="label">Missing</div>
            </div>
        </div>
    </div>
EOF

    # Write comparisons
    echo "$comparisons" >> "$report_path"

    # Footer
    cat >> "$report_path" << 'FOOTER'
    <div class="footer">
        Generated by VHS Terminal Test Harness
    </div>
</body>
</html>
FOOTER

    echo "Report generated: $report_path"
    echo "Results: $passed passed, $failed failed, $missing missing"

    # Return failure if any tests failed or missing
    if [ $failed -gt 0 ] || [ $missing -gt 0 ]; then
        return 1
    fi
    return 0
}
