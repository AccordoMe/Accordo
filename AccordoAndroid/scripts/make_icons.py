#!/usr/bin/env python3
"""Draw the Accordo launcher icon for Android from the same geometry as `public/accordo-logo.svg`.

The mark is two round-capped strokes and a dot ("two strokes leaning in to meet; the dot is the
agreement between them") on a 64×64 grid; the background is the brand gradient. Drawing it here keeps
the phone icon identical to the web's at every density, without an SVG rasteriser.

    python3 AccordoAndroid/scripts/make_icons.py
"""

import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[2]
RES = ROOT / "AccordoAndroid" / "app" / "src" / "main" / "res"

# From accordo-logo.svg (64×64 grid).
STROKE_A = ((16, 48), (30, 16))
STROKE_B = ((48, 48), (34, 16))
STROKE_WIDTH = 6
DOT = ((32, 36), 4.5)
RADIUS_RATIO = 15 / 64  # the <rect rx="15">

STOPS = [(0.0, (0x63, 0x66, 0xF1)), (0.55, (0x8B, 0x5C, 0xF6)), (1.0, (0x38, 0xBD, 0xF8))]

# Legacy launcher sizes (mdpi … xxxhdpi) and the adaptive layers at xxxhdpi.
MIPMAPS = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
ADAPTIVE = 432  # 108dp at 4×
MARK_SHARE = 0.98  # the 64-grid, so the mark's own box (~59% of it) fills the safe zone


def gradient(size: int) -> Image.Image:
    """The 45° indigo→violet→sky gradient of the SVG, drawn small and scaled up (smooth, cheap)."""
    seed = Image.new("RGB", (64, 64))
    px = seed.load()
    assert px is not None
    for y in range(64):
        for x in range(64):
            # The brand gradient across the visible circle, not the whole 108dp canvas: otherwise
            # launchers crop it to the indigo half and the sky-blue corner never shows.
            raw = (x + y) / 126.0
            t = min(1.0, max(0.0, (raw - 0.17) / 0.66))
            for (t0, c0), (t1, c1) in zip(STOPS, STOPS[1:]):
                if t0 <= t <= t1:
                    k = (t - t0) / (t1 - t0) if t1 > t0 else 0.0
                    px[x, y] = tuple(round(a + (b - a) * k) for a, b in zip(c0, c1))
                    break
            else:
                px[x, y] = STOPS[-1][1]
    return seed.resize((size, size), Image.Resampling.BICUBIC)


def draw_mark(size: int, scale_share: float) -> Image.Image:
    """The two strokes and the dot, white, on transparency, centred in a `size` canvas."""
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    scale = size * scale_share / 64.0
    offset = (size - 64 * scale) / 2.0

    def p(point):
        return (offset + point[0] * scale, offset + point[1] * scale)

    width = max(1, round(STROKE_WIDTH * scale))

    solid = ImageDraw.Draw(layer)
    solid.line([p(STROKE_A[0]), p(STROKE_A[1])], fill=(255, 255, 255, 255), width=width)
    for end in STROKE_A:  # round caps, as stroke-linecap="round" gives
        x, y = p(end)
        solid.ellipse([x - width / 2, y - width / 2, x + width / 2, y + width / 2], fill=(255, 255, 255, 255))
    x, y = p(DOT[0])
    r = DOT[1] * scale
    solid.ellipse([x - r, y - r, x + r, y + r], fill=(255, 255, 255, 255))

    faded = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    soft = ImageDraw.Draw(faded)
    soft.line([p(STROKE_B[0]), p(STROKE_B[1])], fill=(255, 255, 255, 191), width=width)  # opacity 0.75
    for end in STROKE_B:
        x, y = p(end)
        soft.ellipse([x - width / 2, y - width / 2, x + width / 2, y + width / 2], fill=(255, 255, 255, 191))

    return Image.alpha_composite(layer, faded)


def rounded_icon(size: int) -> Image.Image:
    """What a launcher shows on API < 26: the rounded square with the mark on it."""
    icon = gradient(size).convert("RGBA")
    icon = Image.alpha_composite(icon, draw_mark(size, 1.0))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=round(size * RADIUS_RATIO), fill=255)
    icon.putalpha(mask)
    return icon


def save(image: Image.Image, *parts: str) -> None:
    target = RES.joinpath(*parts)
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)
    print(f"{target.relative_to(ROOT)}  {image.size[0]}×{image.size[1]}")


def main() -> None:
    for density, size in MIPMAPS.items():
        icon = rounded_icon(size)
        save(icon, f"mipmap-{density}", "ic_launcher.png")
        save(icon, f"mipmap-{density}", "ic_launcher_round.png")

    # Adaptive layers (API 26+): a full-bleed gradient behind, the mark inside the safe zone.
    save(gradient(ADAPTIVE).convert("RGBA"), "drawable-xxxhdpi", "ic_launcher_background.png")
    save(draw_mark(ADAPTIVE, MARK_SHARE), "drawable-xxxhdpi", "ic_launcher_foreground.png")

    adaptive = """<?xml version="1.0" encoding="utf-8"?>
<!-- The brand mark on the brand gradient; the launcher may crop it to any shape. -->
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_foreground" />
</adaptive-icon>
"""
    for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
        target = RES / "mipmap-anydpi-v26" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(adaptive)
        print(f"{target.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
