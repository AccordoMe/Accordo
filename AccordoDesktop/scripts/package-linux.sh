#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A release archive to hand to someone without Rust: the binary, its icon and menu entry, the
# installer and the licences. They unpack it and run ./install.sh.
#   scripts/package-linux.sh     → dist/accordo-<version>-linux-x86_64.tar.gz
set -euo pipefail
cd "$(dirname "$0")/.."
APP_ID=me.accordo.Accordo
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(uname -m)"
cargo build --release
NAME="accordo-$VERSION-linux-$ARCH"
OUT="dist/$NAME"
rm -rf "$OUT" && mkdir -p "$OUT"
cp target/release/accordo-native "$OUT/"
cp assets/linux/$APP_ID.desktop "$OUT/"
cp assets/accordo-logo.svg "$OUT/$APP_ID.svg"
cp assets/linux/accordo-icon-512.png "$OUT/$APP_ID.png"
cp scripts/install-linux.sh "$OUT/install.sh"
cp LICENSE TRADEMARKS.md THIRD_PARTY.md "$OUT/"
GLIBC="$(objdump -T target/release/accordo-native 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -Vu | tail -1 | sed 's/GLIBC_//')"
cat > "$OUT/README.txt" <<TXT
Accordo $VERSION for Linux ($ARCH)

Install (menu entry with the icon, for you only — no sudo):   ./install.sh
For everyone on the computer:                                  ./install.sh --system
Remove:                                                        ./install.sh --uninstall
Or just run it without installing:                             ./accordo-native

Needs glibc $GLIBC or newer (Ubuntu 24.04+, Fedora 40+, Debian 13, Arch…).
No video driver? ICED_BACKEND=tiny-skia ./accordo-native
TXT
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "✓ dist/$NAME.tar.gz ($(du -h "dist/$NAME.tar.gz" | cut -f1))"
