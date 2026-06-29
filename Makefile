# VHS visual snapshot tests (real-terminal screenshots).
#
# Run all tapes for a terminal, a single tape, update/accept goldens, etc.
# Kitty runs in the background (no focus stealing); ghostty/wezterm steal focus.
#
#   make vhs-kitty                      # all tapes, Kitty (recommended)
#   make vhs-kitty TAPE=pdf_smoke       # one tape
#   make vhs TERM=ghostty               # all tapes, a chosen terminal
#   make vhs-kitty OPEN=1               # open the HTML report afterwards
#   make vhs-update TERM=kitty TAPE=pdf_smoke      # re-run + accept all goldens
#   make vhs-accept TERM=kitty TAPE=pdf_smoke      # accept last captures (no re-run)
#   make vhs-accept TERM=kitty TAPE=pdf_smoke SHOT=initial   # accept one snapshot
#   make vhs-list

VHS  := vhs_tests/run.sh
TERM ?= kitty
TAPE ?=
SHOT ?=
OPEN ?=

# Optional-argument helpers.
_tape := $(if $(TAPE),--tape $(TAPE),)
_shot := $(if $(SHOT),--screenshot $(SHOT),)
_open := $(if $(OPEN),--open-report,)

.PHONY: vhs vhs-kitty vhs-ghostty vhs-wezterm vhs-iterm vhs-update vhs-accept vhs-list help

## Run tapes for TERM (default kitty); set TAPE=name for a single tape.
vhs:
	$(VHS) --terminal $(TERM) $(_tape) $(_open)

vhs-kitty:
	$(VHS) --terminal kitty $(_tape) $(_open)

vhs-ghostty:
	$(VHS) --terminal ghostty $(_tape) $(_open)

vhs-wezterm:
	$(VHS) --terminal wezterm $(_tape) $(_open)

vhs-iterm:
	$(VHS) --terminal iterm $(_tape) $(_open)

## Re-run and accept all goldens for TERM (optionally one TAPE).
vhs-update:
	$(VHS) --terminal $(TERM) $(_tape) --update

## Accept already-captured screenshots as golden without re-running.
## Optionally a single SHOT=<screenshot-name>.
vhs-accept:
	$(VHS) --terminal $(TERM) $(_tape) --accept $(_shot)

vhs-list:
	$(VHS) --list

help:
	@sed -n 's/^## //p; /^#   make/s/^#   /  /p' Makefile
