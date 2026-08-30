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

.PHONY: vhs vhs-kitty vhs-ghostty vhs-wezterm vhs-iterm vhs-update vhs-accept vhs-list help \
	pre-release-check lint svg-tests nix-build

## Pre-release gate: lint, SVG snapshot tests, all VHS tapes on kitty +
## wezterm, then the nix build. Fails on the first broken step.
pre-release-check: lint svg-tests
	$(VHS) --terminal kitty
	$(VHS) --terminal wezterm
	$(MAKE) nix-build
	@echo "════════════════════════════════════════════════════════════"
	@echo "pre-release-check: ALL CHECKS PASSED"

## Clippy with warnings denied, both feature configurations.
lint:
	cargo clippy --features pdf -- -D warnings
	cargo clippy -- -D warnings

## SVG snapshot tests; opens the HTML report in the browser.
svg-tests:
	OPEN_REPORT=1 cargo test --features pdf --test svg_snapshots

## Build the nix package. Uses local nix when installed; otherwise falls back
## to the nixos/nix Docker image (start docker/colima first).
nix-build:
	@if command -v nix >/dev/null 2>&1; then \
		nix --extra-experimental-features "nix-command flakes" build; \
	else \
		echo "nix not found locally — building via Docker (nixos/nix)"; \
		docker run --rm -v "$(CURDIR)":/src -w /src nixos/nix \
			nix --extra-experimental-features "nix-command flakes" build /src; \
	fi

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
