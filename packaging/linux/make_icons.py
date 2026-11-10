#!/usr/bin/env python3
"""Generate the Linux icon set for openRTLinux.

    python3 packaging/linux/make_icons.py [--out packaging/linux] [--variant on|off]

The mark is the **key the program lives behind**: a light keycap with a filled
shift arrow on the project's rounded blue tile. It replaced the A/ก tile in
v0.3.0 - two glyphs and an exchange arc turned to mud in a panel, while a keycap
survives 16 px (see `docs/PORTING-LINUX.md`, "The icon").

* every size is drawn at 4x and downsampled, never scaled from 256 px, so the
  16/24 px versions stay sharp in a panel or window list;
* the smallest sizes get their own drawing: no key lip, a bigger cap and a
  thicker arrow, because a 1 px arrowhead is not an arrow;
* a `hicolor` tree ready to install under `~/.local/share/icons` or
  `/usr/share/icons`, plus a hand-written SVG for anything that scales further;
* state variants keep upstream's meanings: `on` (blue), `off` (grey).

Needs Pillow; it finds the fonts itself and says what it used, instead of
assuming a distribution's font layout.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageDraw

# Pillow renamed the resampling constants; keep working on both generations.
LANCZOS = getattr(Image, "Resampling", Image).LANCZOS

ROOT = Path(__file__).resolve().parents[2]

# the tile gradient, and the "off" greys the tray uses when the daemon is stopped
ON = ((0x2E, 0x8B, 0xE6), (0x0A, 0x3D, 0x7A))
OFF = ((0x9A, 0x9A, 0x9A), (0x5E, 0x5E, 0x5E))

# the keycap: light face, a lip under it, and the arrow in the deep blue
CAP_ON, LIP_ON, ARROW_ON = (0xEA, 0xF4, 0xFF), (0x08, 0x20, 0x3C), (0x0A, 0x57, 0xA8)
CAP_OFF, LIP_OFF, ARROW_OFF = (0xEC, 0xEC, 0xEC), (0x1A, 0x1A, 0x1A), (0x55, 0x55, 0x55)

SIZES = [16, 22, 24, 32, 48, 64, 128, 256, 512]

THAI_CANDIDATES = [
    "/usr/share/fonts/truetype/noto/NotoLoopedThai-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansThai-Bold.ttf",
    "/usr/share/fonts/truetype/tlwg/Waree-Bold.ttf",
    "/usr/share/fonts/truetype/tlwg/Garuda-Bold.ttf",
    "/usr/share/fonts/truetype/tlwg/Kinnari-Bold.ttf",
    "/usr/share/fonts/truetype/tlwg/Loma-Bold.ttf",
]
LATIN_CANDIDATES = [
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/tlwg/Garuda-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
]


def find_font(candidates: list[str], what: str) -> str:
    for path in candidates:
        if Path(path).exists():
            return path
    # last resort: anything the system has for that script
    for pattern in ("*Thai*Bold*.ttf", "*Thai*.ttf"):
        hits = sorted(Path("/usr/share/fonts").rglob(pattern))
        if hits:
            return str(hits[0])
    sys.exit(f"no {what} bold font found - tried: {', '.join(candidates)}")


def gradient(size: int, colors) -> Image.Image:
    top, bottom = colors
    grad = Image.new("RGBA", (1, size))
    for y in range(size):
        t = y / (size - 1)
        grad.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    return grad.resize((size, size))


def tile(size: int, colors, cap, lip, arrow) -> Image.Image:
    """The whole icon at `size`, drawn at 4x and downsampled."""
    scale = 4  # supersample for anti-aliased edges
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    margin = 0 if size <= 24 else round(s * 0.04)
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (margin, margin, s - 1 - margin, s - 1 - margin), radius=round(s * 0.24), fill=255
    )
    img.paste(gradient(s, colors), (0, 0), mask)
    draw = ImageDraw.Draw(img, "RGBA")

    if size <= 20:
        # no lip, a bigger cap, a fatter arrow: below ~20 px the detail is noise
        cap_box = (0.11 * s, 0.15 * s, 0.89 * s, 0.71 * s)
        draw.rounded_rectangle(cap_box, radius=round(0.10 * s), fill=cap)
        arrow_scale = 1.14
    else:
        # the lip first, so the cap sits on top of it
        draw.rounded_rectangle(
            (0.172 * s, 0.656 * s, 0.828 * s, 0.758 * s), radius=round(0.051 * s), fill=lip + (140,)
        )
        cap_box = (0.172 * s, 0.195 * s, 0.828 * s, 0.711 * s)
        draw.rounded_rectangle(cap_box, radius=round(0.117 * s), fill=cap)
        arrow_scale = 1.0

    # the shift arrow: a chevron and a stem as one filled polygon (a font glyph
    # of the same shape turns to mush at 16 px)
    cx, cy = 0.5 * s, 0.47 * s
    points = [
        (0.500, 0.289), (0.688, 0.516), (0.586, 0.516), (0.586, 0.648),
        (0.414, 0.648), (0.414, 0.516), (0.312, 0.516),
    ]
    draw.polygon(
        [(cx + (x - 0.5) * s * arrow_scale, cy + (y - 0.47) * s * arrow_scale) for x, y in points],
        fill=arrow,
    )
    return img.resize((size, size), LANCZOS)


SVG = """<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <!-- Mirrors packaging/linux/make_icons.py: the project's rounded blue tile with
       the key the program lives behind. The arrow is a filled path, not a font
       glyph, so it stays a shape at 16 px; the SVG is the same mark the PNGs are
       drawn from, kept here for anything that scales further. -->
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0.4" y2="1">
      <stop offset="0" stop-color="#2E8BE6"/>
      <stop offset="1" stop-color="#0A3D7A"/>
    </linearGradient>
  </defs>
  <rect width="256" height="256" rx="58" fill="url(#tile)"/>
  <rect x="44" y="168" width="168" height="26" rx="13" fill="#08203c" fill-opacity="0.55"/>
  <rect x="44" y="50" width="168" height="132" rx="30" fill="#EAF4FF"/>
  <path d="M128 74 L176 132 H150 V166 H106 V132 H80 Z" fill="#0A57A8"/>
</svg>
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(ROOT / "packaging" / "linux"))
    ap.add_argument("--variant", default="on", choices=["on", "off"])
    args = ap.parse_args()

    out = Path(args.out)
    on = args.variant == "on"
    colors = ON if on else OFF
    cap, lip, arrow = (CAP_ON, LIP_ON, ARROW_ON) if on else (CAP_OFF, LIP_OFF, ARROW_OFF)
    name = "openrtlinux" if on else "openrtlinux-off"

    written = []
    for size in SIZES:
        target = out / "hicolor" / f"{size}x{size}" / "apps" / f"{name}.png"
        target.parent.mkdir(parents=True, exist_ok=True)
        tile(size, colors, cap, lip, arrow).save(target)
        written.append(target)
    # some launchers only ask for one PNG, and the README shows one
    extras = [out / f"{name}.png"]
    if on:
        extras += [out / "openrtlinux-screenshot.png", ROOT / "docs" / "img" / "icon.png"]
    for extra in extras:
        extra.parent.mkdir(parents=True, exist_ok=True)
        tile(256, colors, cap, lip, arrow).save(extra)
        written.append(extra)
    if on:
        svg = out / "openrtlinux.svg"
        svg.write_text(SVG)
        written.append(svg)

    print(f"{len(written)} files written for the {args.variant} variant")
    for path in written:
        print(f"  {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
