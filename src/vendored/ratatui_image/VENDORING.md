# Vendored: ratatui-image

This directory contains a vendored copy of [ratatui-image](https://github.com/benjajaja/ratatui-image) v8.0.1.

## Original Project

- **Project**: ratatui-image
- **Author**: Benjamin Große <ste3ls@gmail.com>
- **Repository**: https://github.com/benjajaja/ratatui-image
- **License**: MIT (see LICENSE file in this directory)
- **Version**: 8.0.1

## Why Vendored?

This library has been vendored into bookokrat with performance-specific modifications that are too application-specific to contribute upstream. The changes include:

1. **Kitty Protocol Compression**: Added zlib compression for Kitty protocol image transmission to reduce bandwidth usage during image rendering
2. **Viewport Rendering Fix**: Fixed viewport cropping for non-tiling protocols to properly handle viewport offsets

## Modifications

All modifications are tracked in the bookokrat git repository. The original git history has been preserved in this document for attribution purposes.

### Performance Changes
- Added flate2 dependency for zlib compression
- Modified `src/protocol/kitty.rs` to compress image data before transmission
- Modified `src/lib.rs` to properly handle viewport cropping for non-Kitty protocols

## Original Git History (Excerpt)

The library was originally created by Benjamin Große and has contributions from multiple developers:

```
083d290 - Benjamin Große, 2 years, 4 months ago : initial commit
9f50412 - Benjamin Große, 2 years, 4 months ago : halfblocks backend, refactor
a7513bb - Benjamin Große, 2 years, 2 months ago : kitty backend
bbfdc4e - Benjamin Große, 2 years, 2 months ago : add binary: image viewer
e31ddee - Benjamin Große, 2 years, 2 months ago : Guess terminal graphics support: Sixel, Kitty, Halfblocks.
0d8f65d - Benjamin Große, 2 years, 1 month ago : rename to ratatui-image, v0.2.0
2c61ee4 - ckaznable, 1 year, 10 months ago : fix Picker.kitty_counter overflow
2714bfb - Benjamin Große, 1 year, 10 months ago : iTerm2 protocol
... (and many more commits)
```

For the complete history, see: https://github.com/benjajaja/ratatui-image/commits/main

## License

This vendored copy maintains the original MIT license. See the LICENSE file in this directory.

## Acknowledgments

Special thanks to Benjamin Große and all contributors to ratatui-image for creating this excellent library that enables terminal image rendering across multiple protocols.
