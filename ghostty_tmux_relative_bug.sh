#!/usr/bin/env bash
set -euo pipefail

# Minimal Ghostty+tmux repro for Kitty graphics relative placements.
#
# Run this inside tmux in Ghostty. It draws a red rectangle as a Kitty graphics
# placement relative to a single Unicode-placeholder anchor cell, then switches
# to a blank tmux window. Expected behavior: the red rectangle disappears with
# the tmux window that owns the anchor cell. Buggy behavior: the red rectangle
# persists over the blank tmux window.

if [[ -z "${TMUX:-}" ]]; then
  echo "Run this inside tmux."
  exit 1
fi

tmux set -p allow-passthrough on >/dev/null 2>&1 || true

RED_RGB_B64="/wAA"
GREEN_RGB_B64="AP8A"

CONTROL_IMAGE_ID=3001
CONTROL_PLACEMENT_ID=3001

TEST_IMAGE_ID=4242
ANCHOR_PLACEMENT_ID=5252
CHILD_PLACEMENT_ID=6262
Z_INDEX="${Z_INDEX:--1}"

tmux_apc() {
  local params="$1"
  local payload="${2:-}"

  if [[ -n "$payload" ]]; then
    printf '\033Ptmux;\033\033_G%s;%s\033\033\\\033\\' "$params" "$payload"
  else
    printf '\033Ptmux;\033\033_G%s\033\033\\\033\\' "$params"
  fi
}

rgb_params() {
  local id="$1"
  printf '%d;%d;%d' "$(((id >> 16) & 255))" "$(((id >> 8) & 255))" "$((id & 255))"
}

placeholder_cell() {
  local image_id="$1"
  local placement_id="$2"
  local image_rgb
  local placement_rgb
  image_rgb="$(rgb_params "$image_id")"
  placement_rgb="$(rgb_params "$placement_id")"

  # U+10EEEE followed by row=0, col=0, id_extra=0 diacritics.
  printf '\033[38;2;%sm\033[58;2;%sm\xF4\x8E\xBB\xAE\xCC\x85\xCC\x85\xCC\x85\033[59m\033[39m' \
    "$image_rgb" \
    "$placement_rgb"
}

target_window="ghostty-relative-blank"
if ! tmux list-windows -F '#W' | grep -Fxq "$target_window"; then
  tmux new-window -d -n "$target_window" \
    "bash -lc 'clear; printf \"Blank tmux window. If a red rectangle is still visible, the relative placement leaked across tmux windows.\\n\"; exec sleep 600'"
fi

clear
tmux_apc 'a=d,d=A,q=2'

cat <<TEXT
Ghostty+tmux Kitty relative-placement repro

This window owns the placeholder anchor cells.
Press Enter below to switch to a blank tmux window.

Expected:
  The red rectangle disappears when tmux switches windows.

Bug:
  The red rectangle remains visible over the blank tmux window.

Z_INDEX=$Z_INDEX

Control placeholder cell (should be owned by tmux text):
TEXT

tmux_apc "a=T,f=24,t=d,s=1,v=1,i=$CONTROL_IMAGE_ID,p=$CONTROL_PLACEMENT_ID,U=1,q=2" "$GREEN_RGB_B64"
placeholder_cell "$CONTROL_IMAGE_ID" "$CONTROL_PLACEMENT_ID"

cat <<TEXT


Relative placement test:
The red rectangle is displayed as placement p=$CHILD_PLACEMENT_ID relative to
the single anchor placeholder cell below (image i=$TEST_IMAGE_ID, parent
placement p=$ANCHOR_PLACEMENT_ID).

Anchor cell:
TEXT

tmux_apc "a=T,f=24,t=d,s=1,v=1,i=$TEST_IMAGE_ID,p=$ANCHOR_PLACEMENT_ID,U=1,c=1,r=1,q=2" "$RED_RGB_B64"
placeholder_cell "$TEST_IMAGE_ID" "$ANCHOR_PLACEMENT_ID"
tmux_apc "a=p,i=$TEST_IMAGE_ID,p=$CHILD_PLACEMENT_ID,P=$TEST_IMAGE_ID,Q=$ANCHOR_PLACEMENT_ID,H=0,V=0,c=56,r=12,z=$Z_INDEX,q=2"

cat <<TEXT


Press Enter to switch to tmux window '$target_window'.
TEXT
read -r _

tmux select-window -t "$target_window"
