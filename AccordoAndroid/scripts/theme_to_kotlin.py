#!/usr/bin/env python3
"""Generate the Compose theme from the desktop client's palette.

The phone must look like the desktop, so the palette is not retyped: it is read from
`AccordoDesktop/src/theme.rs` (itself taken from the web app's CSS variables) and emitted
as Kotlin. Run after any change to the desktop palette:

    python3 AccordoAndroid/scripts/theme_to_kotlin.py
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "AccordoDesktop" / "src" / "theme.rs"
TARGET = ROOT / "AccordoAndroid" / "theme" / "AccordoTheme.kt"

HEX = re.compile(r"pub const ([A-Z0-9_]+): Color = hex\(0x([0-9a-fA-F]{6})\);\s*(?://\s*(.*))?")
RGBA = re.compile(r"pub const ([A-Z0-9_]+): Color = rgba\(0x([0-9a-fA-F]{6}), ([0-9.]+)\);\s*(?://\s*(.*))?")

HEADER = '''// SPDX-License-Identifier: GPL-3.0-or-later
// GENERATED from AccordoDesktop/src/theme.rs — do not edit by hand.
// Regenerate: python3 AccordoAndroid/scripts/theme_to_kotlin.py
//
// The desktop client's palette, name for name (its names come from the web app's CSS variables),
// so a Compose screen can be compared with the desktop window side by side.

package me.accordo.mobile.theme

import androidx.compose.ui.graphics.Color

/** Colors of the Accordo desktop client. */
object AccordoColors {
'''


def camel(name: str) -> str:
    parts = name.lower().split("_")
    return parts[0] + "".join(p.capitalize() for p in parts[1:])


def main() -> None:
    source = SOURCE.read_text(encoding="utf-8")
    entries: dict[str, tuple[str, str]] = {}

    for match in HEX.finditer(source):
        name, rgb, comment = match.group(1), match.group(2), (match.group(3) or "").strip()
        entries[name] = (f"Color(0xFF{rgb.upper()})", comment)
    for match in RGBA.finditer(source):
        name, rgb, alpha, comment = match.group(1), match.group(2), float(match.group(3)), (match.group(4) or "").strip()
        a = round(alpha * 255)
        entries[name] = (f"Color(0x{a:02X}{rgb.upper()})", comment)

    lines = [HEADER]
    for name, (value, comment) in entries.items():
        if comment:
            lines.append(f"    /** {comment} */\n")
        lines.append(f"    val {camel(name)} = {value}   // {name}\n")
    lines.append("}\n")

    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text("".join(lines), encoding="utf-8")
    print(f"{len(entries)} colors → {TARGET.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
