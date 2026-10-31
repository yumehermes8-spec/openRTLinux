#!/usr/bin/env python3
"""Generate the Linux icon set for the RightType port (`rtlinux`).

    python3 packaging/linux/make_icons.py [--out packaging/linux]

Same visual language as the Windows icon in `packaging/icons/make_icons.py`
(the project's own: a rounded blue tile with the two scripts that this whole
program is about), extended for Linux:

* every size is drawn at 4x and downsampled, never scaled from 256 px, so the
  16/24 px versions stay sharp in a panel or window list;
* a `hicolor` tree ready to install under `~/.local/share/icons` or
  `/usr/share/icons`, plus a hand-written SVG for anything that scales further;
* state variants keep upstream's meanings: `on` (blue), `off` (grey).

Needs Pillow and a Thai + a Latin bold face; it finds them itself and says what
it used, instead of assuming a distribution's font layout.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Pillow renamed the resampling constants; keep working on both generations.
LANCZOS = getattr(Image, "Resampling", Image).LANCZOS

ROOT = Path(__file__).resolve().parents[2]

ON = ((0x4C, 0xC2, 0xFF), (0x00, 0x5F, 0xB8))
OFF = ((0x9A, 0x9A, 0x9A), (0x5E, 0x5E, 0x5E))

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


def tile(size: int, colors, thai: str, latin: str, swap: bool) -> Image.Image:
    scale = 4  # supersample for anti-aliased edges
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    top, bottom = colors
    grad = Image.new("RGBA", (1, s))
    for y in range(s):
        t = y / (s - 1)
        grad.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    grad = grad.resize((s, s))

    margin = 0 if size <= 24 else round(s * 0.04)
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (margin, margin, s - 1 - margin, s - 1 - margin), radius=round(s * 0.24), fill=255
    )
    img.paste(grad, (0, 0), mask)
    draw = ImageDraw.Draw(img)
    white = (255, 255, 255, 255)

    if size <= 20:
        # one glyph only: at this size two are mud, and Thai reads as "Thai typing"
        font = ImageFont.truetype(thai, round(s * 0.78))
        draw.text((s / 2, s * 0.53), "ก", font=font, fill=white, anchor="mm")
        return img.resize((size, size), LANCZOS)

    latin_font = ImageFont.truetype(latin, round(s * 0.46))
    thai_font = ImageFont.truetype(thai, round(s * 0.50))
    draw.text((s * 0.33, s * 0.40), "A", font=latin_font, fill=white, anchor="mm")
    draw.text((s * 0.67, s * 0.62), "ก", font=thai_font, fill=white, anchor="mm")

    if swap and size >= 48:
        # the port's own mark: an exchange arc between the two scripts
        width = max(1, round(s * 0.022))
        ink = (255, 255, 255, 190)
        box = (round(s * 0.30), round(s * 0.34), round(s * 0.70), round(s * 0.78))
        draw.arc(box, start=205, end=335, fill=ink, width=width)
        # a small arrowhead on the upper end of the arc
        arrow = round(s * 0.045)
        cx, cy = round(s * 0.70), round(s * 0.55)
        draw.polygon(
            [(cx, cy - arrow), (cx + arrow, cy + arrow // 2), (cx - arrow, cy + arrow // 2)],
            fill=white,
        )
    return img.resize((size, size), LANCZOS)


SVG = """<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <!-- Mirrors packaging/linux/make_icons.py: the project's rounded tile with the
       two scripts it exists to exchange. Text is kept as text so it stays sharp
       at any size; the .ttf used at build time is named in the generator. -->
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#4CC2FF"/>
      <stop offset="1" stop-color="#005FB8"/>
    </linearGradient>
  </defs>
  <rect x="10" y="10" width="236" height="236" rx="61" fill="url(#tile)"/>
  <text x="84" y="118" font-family="Noto Sans, DejaVu Sans, sans-serif" font-size="118"
        font-weight="700" fill="#ffffff" text-anchor="middle">A</text>
  <text x="172" y="176" font-family="Noto Looped Thai, Waree, Garuda, sans-serif"
        font-size="128" font-weight="700" fill="#ffffff" text-anchor="middle">&#3585;</text>
  <path d="M 77 106 A 51 56 0 0 0 179 141" fill="none" stroke="#ffffff"
        stroke-opacity="0.75" stroke-width="6"/>
  <path d="M 179 128 l 14 14 l -26 6 z" fill="#ffffff"/>
</svg>
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(ROOT / "packaging" / "linux"))
    ap.add_argument("--variant", default="on", choices=["on", "off"])
    ap.add_argument("--swap", action="store_true",
                    help="draw the exchange arc (looked cluttered over the A - off by default)")
    args = ap.parse_args()

    thai = find_font(THAI_CANDIDATES, "Thai")
    latin = find_font(LATIN_CANDIDATES, "Latin")
    print(f"thai font : {thai}")
    print(f"latin font: {latin}")

    out = Path(args.out)
    colors = ON if args.variant == "on" else OFF
    swap = args.swap
    name = "rtlinux" if args.variant == "on" else "rtlinux-off"

    written = []
    for size in SIZES:
        img = tile(size, colors, thai, latin, swap)
        target = out / "hicolor" / f"{size}x{size}" / "apps" / f"{name}.png"
        target.parent.mkdir(parents=True, exist_ok=True)
        img.save(target)
        written.append(target)
    # GNOME looks in scalable/ too, and some launchers only ask for one PNG
    img = tile(256, colors, thai, latin, swap)
    for extra in (out / f"{name}.png", out / "righttype-linux.png"):
        extra.parent.mkdir(parents=True, exist_ok=True)
        img.save(extra)
        written.append(extra)
    if args.variant == "on":
        svg = out / "rtlinux.svg"
        svg.write_text(SVG)
        written.append(svg)

    print(f"{len(written)} files written under {out}")
    for path in written:
        print(f"  {path.relative_to(out) if path.is_relative_to(out) else path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
