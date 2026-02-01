# Vendored: tui-textarea

This directory contains a vendored copy of [tui-textarea](https://github.com/rhysd/tui-textarea).

## Source

- **Repository**: https://github.com/rhysd/tui-textarea
- **Version**: 0.7.0 (with modifications from PR #118)
- **License**: MIT

## Why Vendored

The crates.io version of `tui-textarea 0.7.0` depends on `ratatui 0.29.0`, but this project uses `ratatui 0.30.0`. The ratatui 0.30.0 release restructured its crate layout (splitting into `ratatui-core` and `ratatui-widgets`), causing type mismatches.

PR #118 (https://github.com/rhysd/tui-textarea/pull/118) adds ratatui 0.30.0 support but has not been merged/released yet. This vendored copy includes those changes.

## Modifications

1. **Ratatui 0.30.0 compatibility** - Updated imports to work with ratatui 0.30.0's new crate structure
2. **Removed feature gates** - Removed `cfg(feature = "ratatui")`, `cfg(feature = "tuirs")`, `cfg(feature = "search")` since we only use ratatui with search enabled
3. **Removed unused backends** - Removed termion and termwiz input backends (only crossterm is used)
4. **Removed serde/arbitrary derives** - Removed optional serde and arbitrary feature support
5. **Fixed imports** - Changed `crate::` imports to `super::` for use as a submodule

## Updating

To update this vendored code:

1. Check if a new version of tui-textarea with ratatui 0.30+ support has been released
2. If so, consider switching back to the crates.io dependency
3. If not, manually apply any upstream changes needed
