#!/usr/bin/env bash
#
# report_generator.sh - HTML report generation for VHS tests
#
# Generates a self-contained HTML report with embedded images.
# Requires image_compare.sh to be sourced first (done by run.sh)

# Minimal HTML escaping for description text.
html_escape() {
    local s="$1"
    s="${s//&/&amp;}"
    s="${s//</&lt;}"
    s="${s//>/&gt;}"
    printf '%s' "$s"
}

# ─── Aggregate report ────────────────────────────────────────────────────────
#
# One report for ALL tapes run, grouped by tape (each tape is a scenario).
# Images are referenced by RELATIVE PATH (not base64) so the HTML stays small and
# loads fast. The report lives in vhs_tests/output/reports/, so:
#   actual  -> ../screenshots/<term>/<tape>/<name>.png
#   diff    -> ../screenshots/<term>/<tape>/<name>_diff.png
#   golden  -> ../../golden/<term>/<tape>/<name>.png
#
# Usage: generate_aggregate_report TERM REPORT_PATH GOLDEN_ROOT SHOTS_ROOT TAPES_DIR tape...
generate_aggregate_report() {
    local term="$1"
    local report_path="$2"
    local golden_root="$3"
    local shots_root="$4"
    local tapes_dir="$5"
    shift 5
    local tapes=("$@")

    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local grand_pass=0 grand_fail=0 grand_missing=0

    local index_html=""    # top-of-page jump list
    local sections_html="" # per-tape sections

    for tape_name in "${tapes[@]}"; do
        local tape_file="$tapes_dir/$tape_name.tape"
        local golden_dir="$golden_root/$term/$tape_name"
        local actual_dir="$shots_root/$term/$tape_name"
        local actual_rel="../screenshots/$term/$tape_name"
        local golden_rel="../../golden/$term/$tape_name"

        local screenshots=($(grep '^screenshot' "$tape_file" 2>/dev/null | awk '{print $2}'))
        [ ${#screenshots[@]} -eq 0 ] && continue

        local about_html=""
        if [ -f "$actual_dir/_about.txt" ]; then
            about_html="<div class=\"about\">$(html_escape "$(cat "$actual_dir/_about.txt")")</div>"
        fi

        local t_pass=0 t_fail=0 t_missing=0
        local cards=""

        for name in "${screenshots[@]}"; do
            local golden="$golden_dir/$name.png"
            local actual="$actual_dir/$name.png"

            local comp_output=$(compare_images "$golden" "$actual" "$name")
            local status=$(echo "$comp_output" | grep "^STATUS=" | cut -d= -f2)
            local message=$(echo "$comp_output" | grep "^MESSAGE=" | cut -d= -f2-)
            local golden_dims=$(echo "$comp_output" | grep "^GOLDEN_DIMS=" | cut -d= -f2)
            local actual_dims=$(echo "$comp_output" | grep "^ACTUAL_DIMS=" | cut -d= -f2)

            local status_icon status_class
            case "$status" in
                match)    t_pass=$((t_pass+1));    status_icon="✅"; status_class="pass" ;;
                missing)  t_missing=$((t_missing+1)); status_icon="⚠️"; status_class="missing" ;;
                *)        t_fail=$((t_fail+1));    status_icon="❌"; status_class="fail" ;;
            esac

            # Diff only on failure (and only if both images exist).
            local has_diff="false"
            if [ "$status_class" = "fail" ] && [ -f "$golden" ] && [ -f "$actual" ]; then
                if generate_diff_image "$golden" "$actual" "$actual_dir/${name}_diff.png"; then
                    has_diff="true"
                fi
            fi

            local desc_html=""
            if [ -f "$actual_dir/$name.desc.txt" ]; then
                desc_html="<div class=\"desc\">$(html_escape "$(cat "$actual_dir/$name.desc.txt")")</div>"
            fi

            cards+="<div class=\"comparison $status_class\">"
            cards+="<div class=\"comparison-header\">"
            cards+="<span class=\"status\">$status_icon</span>"
            cards+="<span class=\"name\">$name</span>"
            cards+="<span class=\"message\">$message</span>"
            cards+="</div>"
            [ -n "$desc_html" ] && cards+="$desc_html"

            if [ "$has_diff" = "true" ]; then
                cards+="<div class=\"comparison-images has-diff\">"
            else
                cards+="<div class=\"comparison-images\">"
            fi

            cards+="<div class=\"image-box\"><div class=\"label\">Expected (Golden)</div>"
            if [ -f "$golden" ]; then
                cards+="<img loading=\"lazy\" src=\"$golden_rel/$name.png\" alt=\"golden $name\">"
            else
                cards+="<div class=\"placeholder\">No golden snapshot</div>"
            fi
            cards+="</div>"

            cards+="<div class=\"image-box\">"
            if [ "$has_diff" = "true" ]; then
                cards+="<div class=\"label\">Actual (with diff)</div>"
                cards+="<img loading=\"lazy\" src=\"$actual_rel/${name}_diff.png\" alt=\"diff $name\">"
            else
                cards+="<div class=\"label\">Actual</div>"
                if [ -f "$actual" ]; then
                    cards+="<img loading=\"lazy\" src=\"$actual_rel/$name.png\" alt=\"actual $name\">"
                else
                    cards+="<div class=\"placeholder\">No screenshot captured</div>"
                fi
            fi
            cards+="</div>"

            if [ "$has_diff" = "true" ]; then
                cards+="<div class=\"image-box\"><div class=\"label\">Actual (clean)</div>"
                cards+="<img loading=\"lazy\" src=\"$actual_rel/$name.png\" alt=\"actual $name\"></div>"
            fi
            cards+="</div>"

            if [ -n "$golden_dims" ] || [ -n "$actual_dims" ]; then
                cards+="<div class=\"comparison-details\">"
                [ -n "$golden_dims" ] && cards+="<span>Golden: $golden_dims</span>"
                [ -n "$actual_dims" ] && cards+="<span>Actual: $actual_dims</span>"
                cards+="</div>"
            fi

            if [ "$status_class" != "pass" ]; then
                local accept_one="./vhs_tests/run.sh --terminal $term --tape $tape_name --accept --screenshot $name"
                cards+="<div class=\"update-cmd\"><code>$accept_one</code>"
                cards+="<button class=\"copy-btn\" onclick=\"navigator.clipboard.writeText('$accept_one')\">📋 Accept This</button></div>"
            fi

            cards+="</div>"
        done

        grand_pass=$((grand_pass + t_pass))
        grand_fail=$((grand_fail + t_fail))
        grand_missing=$((grand_missing + t_missing))

        # Tape-level status + whether to auto-expand (open on any fail/missing).
        local tape_state="pass"; local open_attr=""
        if [ $t_fail -gt 0 ] || [ $t_missing -gt 0 ]; then tape_state="fail"; open_attr="open"; fi
        local badge="<span class=\"pill ok\">$t_pass ✓</span>"
        [ $t_fail -gt 0 ] && badge+="<span class=\"pill bad\">$t_fail ✗</span>"
        [ $t_missing -gt 0 ] && badge+="<span class=\"pill warn\">$t_missing ?</span>"

        local accept_all="./vhs_tests/run.sh --terminal $term --tape $tape_name --accept"

        index_html+="<li class=\"$tape_state\"><a href=\"#tape-$tape_name\">$tape_name</a> $badge</li>"

        sections_html+="<details id=\"tape-$tape_name\" class=\"tape $tape_state\" $open_attr>"
        sections_html+="<summary><span class=\"tname\">$tape_name</span> $badge</summary>"
        sections_html+="$about_html"
        sections_html+="<div class=\"update-cmd\" style=\"margin:10px 0\"><code>$accept_all</code>"
        sections_html+="<button class=\"copy-btn\" onclick=\"navigator.clipboard.writeText('$accept_all')\">📋 Accept ALL in $tape_name</button></div>"
        sections_html+="$cards"
        sections_html+="</details>"
    done

    cat > "$report_path" << HEADER
<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>VHS Report — $term</title>
<style>
:root{--bg:#1a1a2e;--bg-card:#16213e;--bg-hover:#1f2b47;--text:#eee;--text-dim:#888;--pass:#4ade80;--fail:#f87171;--warn:#fbbf24;--border:#334155;}
*{box-sizing:border-box;}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,monospace;background:var(--bg);color:var(--text);margin:0;padding:20px;line-height:1.5;}
.header{text-align:center;margin-bottom:20px;padding:20px;background:var(--bg-card);border-radius:8px;border:1px solid var(--border);}
.header h1{margin:0 0 8px;font-size:22px;}
.header .meta{color:var(--text-dim);font-size:13px;}
.summary{display:flex;justify-content:center;gap:30px;margin:16px 0;}
.summary-item{text-align:center;}
.summary-item .count{font-size:30px;font-weight:bold;}
.summary-item .label{font-size:11px;color:var(--text-dim);text-transform:uppercase;}
.summary-item.pass .count{color:var(--pass);}
.summary-item.fail .count{color:var(--fail);}
.summary-item.missing .count{color:var(--warn);}
.index{background:var(--bg-card);border:1px solid var(--border);border-radius:8px;padding:12px 20px;margin-bottom:20px;}
.index ul{list-style:none;margin:0;padding:0;columns:2;}
.index li{padding:4px 0;}
.index li a{color:var(--text);text-decoration:none;font-weight:bold;}
.index li a:hover{text-decoration:underline;}
.index li.fail a{color:var(--fail);}
.pill{display:inline-block;font-size:11px;padding:1px 7px;border-radius:10px;margin-left:6px;}
.pill.ok{background:rgba(74,222,128,.15);color:var(--pass);}
.pill.bad{background:rgba(248,113,113,.15);color:var(--fail);}
.pill.warn{background:rgba(251,191,36,.15);color:var(--warn);}
details.tape{background:var(--bg-card);border:1px solid var(--border);border-radius:8px;margin-bottom:14px;overflow:hidden;}
details.tape.fail{border-left:4px solid var(--fail);}
details.tape.pass{border-left:4px solid var(--pass);}
details.tape>summary{cursor:pointer;padding:14px 20px;font-size:17px;user-select:none;}
details.tape .tname{font-weight:bold;}
.about{padding:0 20px 8px;color:var(--text);opacity:.85;font-size:14px;}
.desc{padding:10px 20px 0;color:var(--text-dim);font-size:13px;line-height:1.45;}
.comparison{border-top:1px solid var(--border);margin:0;}
.comparison-header{padding:12px 20px;display:flex;align-items:center;gap:12px;}
.comparison-header .name{font-weight:bold;}
.comparison-header .message{color:var(--text-dim);font-size:13px;margin-left:auto;}
.comparison-images{display:grid;grid-template-columns:1fr 1fr;gap:16px;padding:16px 20px;}
.comparison-images.has-diff{grid-template-columns:1fr 1fr 1fr;}
.image-box{text-align:center;}
.image-box .label{font-size:11px;color:var(--text-dim);text-transform:uppercase;margin-bottom:8px;}
.image-box img{max-width:100%;height:auto;border:1px solid var(--border);border-radius:4px;background:#000;}
.image-box .placeholder{padding:40px;background:var(--bg);border:1px dashed var(--border);border-radius:4px;color:var(--text-dim);}
.comparison-details{padding:0 20px 12px;font-size:12px;color:var(--text-dim);display:flex;gap:24px;}
.update-cmd{padding:8px 20px 14px;}
.update-cmd code{display:block;background:#000;padding:8px;border-radius:4px;font-size:12px;overflow-x:auto;}
.copy-btn{margin-top:8px;padding:6px 14px;background:var(--bg-card);border:1px solid var(--border);border-radius:4px;color:var(--text);cursor:pointer;font-size:12px;}
.copy-btn:hover{background:var(--bg-hover);}
.footer{text-align:center;padding:20px;color:var(--text-dim);font-size:12px;}
</style></head><body>
<div class="header">
  <h1>🎬 VHS Report — $term</h1>
  <div class="meta">Generated: $timestamp &nbsp;|&nbsp; ${#tapes[@]} tape(s)</div>
  <div class="summary">
    <div class="summary-item pass"><div class="count">$grand_pass</div><div class="label">Passed</div></div>
    <div class="summary-item fail"><div class="count">$grand_fail</div><div class="label">Failed</div></div>
    <div class="summary-item missing"><div class="count">$grand_missing</div><div class="label">Missing</div></div>
  </div>
</div>
<div class="index"><ul>$index_html</ul></div>
HEADER

    echo "$sections_html" >> "$report_path"
    cat >> "$report_path" << 'FOOTER'
<div class="footer">Generated by VHS Terminal Test Harness</div>
</body></html>
FOOTER

    echo "Aggregate report: $report_path"
    echo "Totals: $grand_pass passed, $grand_fail failed, $grand_missing missing"
    [ $grand_fail -gt 0 ] || [ $grand_missing -gt 0 ] && return 1
    return 0
}
