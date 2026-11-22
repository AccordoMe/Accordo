#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Installs Accordo as an app: the menu entry with its icon, launched with a click.
#
#   scripts/install-linux.sh              for you (~/.local — no sudo)
#   scripts/install-linux.sh --system     for everyone on this computer (/usr/local, asks for sudo)
#   scripts/install-linux.sh --link       for development: a link to target/release, so every
#                                         `cargo build --release` is what the menu starts
#   scripts/install-linux.sh --uninstall  takes it away again (add --system if installed that way)
#
# Works from the source tree (after `cargo build --release`) and from the release archive made by
# scripts/package-linux.sh (the files side by side). Your accounts and settings (~/.config) stay.
set -euo pipefail
APP_ID=me.accordo.Accordo
HERE="$(cd "$(dirname "$0")" && pwd)"

PREFIX="$HOME/.local"; SUDO=""; UNINSTALL=0; LINK=0
for a in "$@"; do
  case "$a" in
    --system) PREFIX=/usr/local; SUDO=sudo ;;
    --uninstall) UNINSTALL=1 ;;
    --link) LINK=1 ;;
    *) echo "Unknown option: $a"; exit 2 ;;
  esac
done
BIN="$PREFIX/bin/accordo-native"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"

refresh() {
  command -v update-desktop-database >/dev/null && $SUDO update-desktop-database -q "$APPS" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && $SUDO gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
  # KDE's menu cache (it notices new entries by itself too); never let it hold the install up.
  for k in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$k" >/dev/null; then timeout 15 "$k" >/dev/null 2>&1 || true; break; fi
  done
}

if [ "$UNINSTALL" = 1 ]; then
  $SUDO rm -f "$BIN" "$APPS/$APP_ID.desktop" "$ICONS/scalable/apps/$APP_ID.svg" "$ICONS/512x512/apps/$APP_ID.png"
  refresh
  echo "✓ Accordo removed (your accounts and settings in ~/.config are kept)"
  exit 0
fi

# The files: next to this script (release archive) or in the source tree.
find_file() { for p in "$@"; do [ -f "$p" ] && { echo "$p"; return; }; done; }
SRC_BIN="$(find_file "$HERE/accordo-native" "$HERE/../target/release/accordo-native")"
DESKTOP="$(find_file "$HERE/$APP_ID.desktop" "$HERE/../assets/linux/$APP_ID.desktop")"
SVG="$(find_file "$HERE/$APP_ID.svg" "$HERE/../assets/accordo-logo.svg")"
PNG="$(find_file "$HERE/$APP_ID.png" "$HERE/../assets/linux/accordo-icon-512.png")"
if [ -z "$SRC_BIN" ]; then
  echo "✗ No accordo-native here. In the source tree build it first: cargo build --release"; exit 1
fi

if [ "$LINK" = 1 ]; then
  BUILT="$(cd "$HERE/.." && pwd)/target/release/accordo-native"
  [ -f "$BUILT" ] || { echo "✗ --link needs the source tree with a release build: cargo build --release"; exit 1; }
  $SUDO mkdir -p "$(dirname "$BIN")"
  $SUDO rm -f "$BIN"
  $SUDO ln -s "$BUILT" "$BIN"
else
  $SUDO rm -f "$BIN"  # a link from --link isn't overwritten through: replaced
  $SUDO install -Dm755 "$SRC_BIN" "$BIN"
fi
$SUDO install -Dm644 "$SVG" "$ICONS/scalable/apps/$APP_ID.svg"
[ -n "$PNG" ] && $SUDO install -Dm644 "$PNG" "$ICONS/512x512/apps/$APP_ID.png"
# The entry runs the installed binary by its full path: no need for ~/.local/bin in PATH.
tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT
sed "s|^Exec=.*|Exec=$BIN|" "$DESKTOP" > "$tmp"
$SUDO install -Dm644 "$tmp" "$APPS/$APP_ID.desktop"
refresh
if [ "$LINK" = 1 ]; then
  echo "✓ Accordo linked to $BUILT — each cargo build --release is what the menu starts"
else
  echo "✓ Accordo installed: find it in the app menu (or run accordo-native)"
fi
