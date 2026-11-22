#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Two windows side by side (left and right half of the screen) — sign in as a different user in each to test chats, cheques,
# invoices between them. Each window keeps its own settings and accounts (ACCORDO_PROFILE):
# the first is your usual one, the second ~/.config/accordo-desktop-two.
#
#   scripts/dev-two.sh             a debug build
#   scripts/dev-two.sh --release   a release build
#   A=alice B=bob scripts/dev-two.sh   your own profile names (A empty = your usual settings)
#   NO_TILE=1 scripts/dev-two.sh       no side by side (plain Wayland windows)
# Ctrl+C closes both.
set -euo pipefail
cd "$(dirname "$0")/.."
DIR=debug; BUILD=(cargo build)
if [ "${1:-}" = --release ]; then DIR=release; BUILD=(cargo build --release); fi
"${BUILD[@]}"
BIN="target/$DIR/accordo-native"
trap 'kill $(jobs -p) 2>/dev/null || true' INT TERM EXIT
# Side by side: each window takes half the screen (ACCORDO_TILE). Wayland doesn't let an app place
# its own window, so on a Wayland desktop these two run through XWayland (X11), where it may: an
# empty WAYLAND_DISPLAY (not unset — libwayland would then try "wayland-0") makes winit and wgpu
# both take X11.
# NO_TILE=1: plain windows on Wayland, placed by the desktop (Super+←/→ to split them).
LEFT=left; RIGHT=right
if [ -n "${NO_TILE:-}" ]; then X11=(env); LEFT=; RIGHT=
elif [ -n "${WAYLAND_DISPLAY:-}" ] && [ -n "${DISPLAY:-}" ]; then X11=(env WAYLAND_DISPLAY=)
else X11=(env); fi
"${X11[@]}" ACCORDO_TILE="$LEFT" ACCORDO_PROFILE="${A:-}" "$BIN" &
"${X11[@]}" ACCORDO_TILE="$RIGHT" ACCORDO_PROFILE="${B:-two}" "$BIN" &
wait
