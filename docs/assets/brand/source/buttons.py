# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
#
# Puts the README's download buttons together: a glass render of each mark and
# name (glass.html, recorded with record.mjs into `glass/<name>.png`) on a
# rounded card, with a line under it. Needs Pillow and the macOS system font.
#
#   python3 buttons.py glass ../../readme/download
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

BUTTONS = {
    "mac": "Download · macOS 14+",
    "windows": "Download · Windows 11",
    "linux": "Download · deb, rpm, AppImage",
}
SCALE = 2
W, H = 560 * SCALE, 160 * SCALE
RADIUS = 30 * SCALE
BACKGROUND = (5, 5, 7, 255)  # the glass shader's own background
BORDER = (139, 124, 246, 110)  # the accent, faint
CAPTION = (157, 152, 179, 255)  # the banner's tagline grey


def button(glass: Image.Image, caption: str) -> Image.Image:
    card = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    shape = Image.new("L", (W, H), 0)
    ImageDraw.Draw(shape).rounded_rectangle((0, 0, W - 1, H - 1), RADIUS, fill=255)
    fill = Image.new("RGBA", (W, H), BACKGROUND)
    fill.paste(glass.convert("RGBA"), ((W - glass.width) // 2, 2 * SCALE))
    card.paste(fill, (0, 0), shape)
    draw = ImageDraw.Draw(card)
    draw.rounded_rectangle((1, 1, W - 2, H - 2), RADIUS, outline=BORDER, width=SCALE)
    font = ImageFont.truetype("/System/Library/Fonts/SFNS.ttf", 26 * SCALE)
    # Axes: width, optical size, grade, weight.
    font.set_variation_by_axes([100, 17, 400, 510])
    draw.text((W / 2, H - 32 * SCALE), caption, font=font, fill=CAPTION, anchor="mm")
    return card


def main(source: str, out: str) -> None:
    Path(out).mkdir(parents=True, exist_ok=True)
    for name, caption in BUTTONS.items():
        glass = Image.open(Path(source) / f"{name}.png")
        button(glass, caption).save(Path(out) / f"{name}.png", optimize=True)


if __name__ == "__main__":
    main(*sys.argv[1:3])
