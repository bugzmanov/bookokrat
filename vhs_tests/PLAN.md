# VHS-Style Terminal Screenshot Test Harness

Self-contained visual regression testing for terminal rendering.

## Directory Structure

```
vhs_tests/
├── PLAN.md                    # This file
├── run.sh                     # Main entry point
├── lib/
│   ├── tape_runner.sh         # Tape parsing and execution
│   ├── window_manager.sh      # Ghostty window handling (macOS)
│   ├── image_compare.sh       # Screenshot comparison
│   └── report_generator.sh    # HTML report generation
├── tapes/
│   └── pdf_smoke.tape         # Test tape definitions
├── golden/
│   └── pdf_smoke/             # Golden snapshots per tape
│       ├── initial.png
│       ├── page_2.png
│       └── ...
├── output/                    # Generated (gitignored)
│   ├── screenshots/           # Actual captures
│   └── reports/               # HTML reports
└── testdata/                  # Test fixtures (or symlink to tests/testdata)
```

## Usage

```bash
# Run all tapes
./vhs_tests/run.sh

# Run specific tape
./vhs_tests/run.sh --tape pdf_smoke

# Update golden snapshots for a tape
./vhs_tests/run.sh --tape pdf_smoke --update

# List available tapes
./vhs_tests/run.sh --list

# Open report after run
./vhs_tests/run.sh --open-report
```

## Tape Format

```tape
# vhs_tests/tapes/pdf_smoke.tape
# Comments start with #

# Initial render
screenshot initial

# Navigate to next page
key j
wait 500
screenshot page_2

# Zen mode toggle
key z
wait 300
screenshot zen_mode
key z

# Help popup
key ?
wait 500
screenshot help_popup
escape
wait 200
screenshot after_help

# Quit
key q
```

### Tape Commands

| Command | Description | Example |
|---------|-------------|---------|
| `screenshot <name>` | Capture window to `<name>.png` | `screenshot initial` |
| `key <char>` | Send keystroke | `key j`, `key ?` |
| `escape` | Send Escape key | `escape` |
| `return` | Send Return/Enter key | `return` |
| `wait <ms>` | Wait N milliseconds | `wait 500` |
| `# comment` | Ignored line | `# Navigate next` |

## HTML Report

Generated at `vhs_tests/output/reports/<tape>_report.html`:

```
┌─────────────────────────────────────────────────────────────┐
│  VHS Terminal Test Report                                    │
│  Tape: pdf_smoke.tape                                        │
│  Date: 2024-01-19 10:30:00                                   │
│  Results: 6/8 passed (2 failures)                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ✅ initial                                                  │
│  ┌──────────────────┐  ┌──────────────────┐                 │
│  │    [Expected]    │  │    [Actual]      │                 │
│  └──────────────────┘  └──────────────────┘                 │
│                                                              │
│  ❌ zen_mode (MISMATCH)                                      │
│  ┌──────────────────┐  ┌──────────────────┐                 │
│  │    [Expected]    │  │    [Actual]      │  [Diff overlay] │
│  └──────────────────┘  └──────────────────┘                 │
│  Dimensions: 1200x800 vs 1200x800                           │
│  Size diff: 15.2%                                            │
│                                                              │
│  [📋 Copy update command]                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Image Comparison Strategy

1. **Dimension check** - Must match exactly
2. **File size heuristic** - Quick fail if >10% different
3. **Future**: Perceptual diff with threshold (SSIM or pixelmatch)

## Implementation Steps

### Phase 1: Core Runner
- [ ] Create directory structure
- [ ] Implement `run.sh` (argument parsing, orchestration)
- [ ] Implement `tape_runner.sh` (parse tape, execute commands)
- [ ] Implement `window_manager.sh` (reuse existing Ghostty logic)
- [ ] Implement `image_compare.sh` (dimension + size comparison)

### Phase 2: Reporting
- [ ] Implement `report_generator.sh` (HTML generation)
- [ ] Embed images as base64 in report (self-contained HTML)
- [ ] Add copy-to-clipboard for update commands

### Phase 3: First Tape
- [ ] Create `pdf_smoke.tape` with basic scenarios
- [ ] Generate initial golden snapshots
- [ ] Verify full workflow

### Phase 4: Polish
- [ ] Add `--open-report` flag
- [ ] Add summary output to terminal
- [ ] Add `--verbose` flag for debugging
- [ ] Consider perceptual image diff

## Dependencies

- macOS (uses `screencapture`, `osascript`)
- Ghostty.app in /Applications
- Swift (for CGWindowList APIs)
- Bash 3+ (macOS compatible)

## Git Integration

Add to `.gitignore`:
```
vhs_tests/output/
```

Golden snapshots in `vhs_tests/golden/` should be committed.
