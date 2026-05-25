# Ghostty Kitty graphics bugs seen in Bookokrat

## Ghostty + tmux: relative placements are not anchored to Unicode placeholders

Bookokrat's Kitty+tmux fix uses the Kitty graphics protocol's intended tmux
ownership model:

1. Upload the PDF page through SHM.
2. Create a tiny virtual placement with `U=1` at the page's top-left cell.
3. Draw that virtual placement as a Kitty Unicode placeholder in the ratatui
   buffer, so tmux owns the image through normal text-grid cells.
4. Display the real PDF page as a relative placement with
   `P=<image_id>,Q=<anchor_placement_id>,H=0,V=0,z=-1`.

This works in Kitty inside tmux. It does not work reliably in Ghostty inside
tmux. Ghostty appears to parse the relative-placement keys (`P`, `Q`, `H`,
`V`) but does not actually bind the child placement's lifetime/position to
the parent placeholder placement. In Bookokrat that means the tmux-safe
relative-anchor path cannot be used as a Ghostty workaround: the image either
does not render correctly or remains owned by the outer Ghostty surface rather
than the tmux window/pane text that should own the anchor.

Minimal local reproducer:

```bash
./ghostty_tmux_relative_bug.sh
```

Run it inside tmux in Ghostty. Expected behavior is that the red relative
placement disappears when switching to the blank tmux window because the
source window owns the placeholder anchor. Buggy behavior is that the relative
placement is not clipped/owned with the tmux window.

Implementation notes from the investigation:

- Ghostty has historical relative-placement discussion in
  `ghostty-org/ghostty#720`, closed by `ghostty-org/ghostty#2015`.
- Later Ghostty code parses `P/Q/H/V`, but the placement execution/storage path
  still only tracks normal and virtual placement state, not a live parent-child
  placement relationship.
- Because this is terminal-side protocol behavior, Bookokrat should keep the
  Kitty+tmux relative-anchor implementation and document Ghostty+tmux as
  currently blocked rather than adding a second, fragile rendering path.

## Ghostty bug: `d=r` / `d=R` does not respect its `x`/`y` ID-range bounds

## Summary

The Kitty graphics protocol delete-by-id-range command
(`\x1b_Ga=d,d=R,x=<min>,y=<max>\x1b\\`, or its lowercase `d=r` variant) is
specified to delete only images whose `i=` is in the inclusive range
`[x, y]`. In Ghostty the bounds check is **tautological**: it matches every
u32 `image_id` regardless of `x` and `y`. The visible effect depends on
whether the matching images currently have placements — see "Effect"
below — but in every case the `x`/`y` keys are effectively ignored.

## Affected version

Latest `main` (`67b5783bdd`, 2026-04-25). The bug has been present since
PR #5957 was merged on 2025-02-24 and is unchanged on `main` today.

## Bug location

[`src/terminal/kitty/graphics_storage.zig:416`](https://github.com/ghostty-org/ghostty/blob/main/src/terminal/kitty/graphics_storage.zig#L416):

```zig
.range => |v| range: {
    if (v.first <= 0 or v.last <= 0) { ... }
    if (v.first > v.last) { ... }                 // <-- v.first <= v.last from here on

    var it = self.placements.iterator();
    while (it.next()) |entry| {
        if (entry.key_ptr.image_id >= v.first or entry.key_ptr.image_id <= v.last) {
        //                                  ^^ should be `and`
            const image_id = entry.key_ptr.image_id;
            entry.value_ptr.deinit(t.screens.active);
            self.placements.removeByPtr(entry.key_ptr);
            if (v.delete) self.deleteIfUnused(alloc, image_id);
        }
    }
    self.dirty = true;
},
```

## Why the predicate is tautological

`v.first <= v.last` is enforced two lines above, so for any `u32` image_id at
least one of the two halves of the disjunction is always true:

* if `image_id >= v.first` is false, then `image_id < v.first <= v.last`, so
  `image_id <= v.last` is true.
* otherwise the first half is already true.

With `or`, the predicate `image_id ∈ [first, last]` is replaced by
`image_id ∈ [0, u32::MAX]`. The `x` and `y` keys are accepted but contribute
nothing.

## Effect

The bounds check itself never filters anything out, but what is visible to
the caller depends on what the loop iterates over (`self.placements`):

| State of the matched images | Visible behaviour | Matches the spec? |
|---|---|---|
| At least one placement exists for some image (e.g. transmitted with `a=T` or via a separate `a=p`) | Every placement is removed and `deleteIfUnused` then frees every image — `d=R` degenerates to `d=A` (delete all) | No |
| No placements exist (images transmitted with `a=t` only) | Loop body never runs; nothing is deleted, including images whose IDs really are in `[x, y]` | No |

Either way, the `x`/`y` range is ignored. The first case is what bookokrat
hits in production: every `d=R` issued by one pane wipes every image (and
in tmux, every pane's images, since they share one Ghostty surface).

> Note: the second case also points at a separate issue — the spec text
> ("Delete all images whose id is …") implies the iteration target should
> be the image map, not the placement map. Kitty's reference
> implementation removes images regardless of whether they have a
> placement; Ghostty does not. Fixing the `or`/`and` typo addresses the
> first row of the table; the second row would still be wrong.

## Impact

Visible whenever multiple Kitty-graphics clients share one Ghostty surface,
most commonly **Ghostty + tmux with multiple panes**. A range delete from any
pane silently wipes images placed by every other pane on the same surface.

In our case (a TUI PDF reader using namespaced image IDs per process so each
process can clean up its own range without touching others'), the bug surfaces
as `ENOENT: image not found` for previously-uploaded pages in pane A every
time pane B issues any range delete (probe, cache-window shrink, mode switch,
shutdown). The exact pattern in our log:

```
20:54:51 [pane B] registered: /bookokrat_0-34245-page-98 ...     # B uploads
20:54:52 [pane A] Kitty response error for image 2337800291 (page 98): ENOENT
```

Native Ghostty splits are unaffected because each split is its own surface,
so a delete-all is contained to the calling pane.

Kitty (the reference implementation) uses the correct `and` and is not
affected.

## Reproducer (bash)

Uses raw RGB transmission (`f=24`) to avoid PNG-decoder differences. Payload
is one base64-encoded 1×1 red pixel (`\xff\x00\x00` → `/wAA`). Crucially,
each image is sent with `a=T` (transmit *and* display) so the buggy code
path — which iterates `self.placements` — actually has placements to look
at. Transmits two images at `i=100` and `i=200`, issues `d=R,x=99,y=101`
(a range whose only valid hit is `100`), then probes both with `a=p`.

```bash
#!/usr/bin/env bash
RGB='/wAA'   # 1x1 red pixel, raw RGB, base64

# Transmit AND display two images (a=T creates a placement, which is what
# Ghostty's d=R iterates over).
printf '\x1b_Ga=T,f=24,t=d,s=1,v=1,i=100,p=100,c=10,r=5;%s\x1b\\' "$RGB"
printf '\n'
printf '\x1b_Ga=T,f=24,t=d,s=1,v=1,i=200,p=200,c=10,r=5;%s\x1b\\' "$RGB"
printf '\n'

# Delete by id range that should cover ONLY id=100.
printf '\x1b_Ga=d,d=R,x=99,y=101\x1b\\'

# Probe both images. With a correct implementation, i=200 still exists.
printf '\x1b_Ga=p,i=100,p=100,c=10,r=5\x1b\\'
printf '\n'
printf '\x1b_Ga=p,i=200,p=200,c=10,r=5\x1b\\'
printf '\n'
```

### Expected (Kitty)

* one red square remains visible on screen (the `i=200` placement)
* response for `i=100` placement: `ENOENT` (image was inside `[99,101]`)
* response for `i=200` placement: `OK` (image was outside `[99,101]`)

### Actual (Ghostty `main` 2026-04-25)

* both red squares disappear after the `d=R`
* response for `i=100` placement: `ENOENT`
* response for `i=200` placement: `ENOENT` — bug: the buggy `or` matched
  every placement and `deleteIfUnused` then freed every image

### Why the obvious bash variant (a=t instead of a=T) does *not* show the bug

If you transmit with `a=t` (transmit only, no display), Ghostty stores the
images in `self.images` but never adds entries to `self.placements`. The
range-delete loop iterates `self.placements`, finds it empty, and does
nothing — so both images survive even though one of them is in range. This
is a *separate* Ghostty issue (the spec text says "Delete all images whose
id is …", so the iteration target should be `self.images`, not
`self.placements`). The reproducer above uses `a=T` to sidestep this and
exercise the OR/AND bug specifically.

## Reproducer (Python, with PASS/FAIL summary)

Same logic, but runs in raw mode and parses replies so it prints a
verdict rather than dumping raw escape sequences.

```python
#!/usr/bin/env python3
"""Reproducer for ghostty d=R range-delete bug. Run from a Ghostty terminal."""
import sys, os, termios, tty, time, select, re

RGB = "/wAA"  # 1x1 red pixel, raw RGB, base64

def w(s):
    sys.stdout.write(s); sys.stdout.flush()

def read_apc(timeout=1.0):
    """Block until one full APC reply (\\x1b_G…\\x1b\\\\) arrives."""
    deadline = time.time() + timeout
    buf = b""
    while time.time() < deadline:
        r, _, _ = select.select([sys.stdin], [], [], max(0.0, deadline - time.time()))
        if not r:
            continue
        buf += os.read(sys.stdin.fileno(), 4096)
        m = re.search(rb"\x1b_G([^\x1b]*)\x1b\\", buf)
        if m:
            _, _, msg = m.group(1).partition(b";")
            return msg.decode("utf-8", errors="replace")
    return "<no response>"

fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
tty.setraw(fd)
try:
    # a=T transmits AND creates a placement; the buggy loop iterates placements.
    w(f"\x1b_Ga=T,f=24,t=d,s=1,v=1,i=100,p=100,c=10,r=5;{RGB}\x1b\\"); r = read_apc()
    print(f"  transmit+display i=100  -> {r}", file=sys.stderr)
    w(f"\x1b_Ga=T,f=24,t=d,s=1,v=1,i=200,p=200,c=10,r=5;{RGB}\x1b\\"); r = read_apc()
    print(f"  transmit+display i=200  -> {r}", file=sys.stderr)

    w("\x1b_Ga=d,d=R,x=99,y=101\x1b\\"); r = read_apc()
    print(f"  d=R x=99 y=101          -> {r}", file=sys.stderr)

    w("\x1b_Ga=p,i=100,p=100,C=1\x1b\\"); r100 = read_apc()
    print(f"  probe i=100             -> {r100}   (expected: ENOENT — was in range)", file=sys.stderr)
    w("\x1b_Ga=p,i=200,p=200,C=1\x1b\\"); r200 = read_apc()
    print(f"  probe i=200             -> {r200}   (expected: OK — outside range)", file=sys.stderr)
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)

print(file=sys.stderr)
if r200 == "OK":
    print("PASS: range delete respected its bounds.", file=sys.stderr)
else:
    print(f"FAIL: id=200 was wiped ({r200}); range delete deleted everything.",
          file=sys.stderr)
    print("Bug at graphics_storage.zig:416 (`or` should be `and`).", file=sys.stderr)
```

Save as `repro.py` and run `python3 repro.py` in a Ghostty window.

## Suggested fix

```diff
-                    if (entry.key_ptr.image_id >= v.first or entry.key_ptr.image_id <= v.last) {
+                    if (entry.key_ptr.image_id >= v.first and entry.key_ptr.image_id <= v.last) {
```

Worth adding a regression test that transmits two images at distinct IDs,
deletes a range covering only one of them, and asserts the other is still
in `placements`. The original PR #5957 only tested the positive case.
