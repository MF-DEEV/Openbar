#!/usr/bin/env python3
"""Turns the Blender renders into the apps' companion sprites and art tables.

    python3 Tools/companion/build_assets.py SPRITES_DIR

SPRITES_DIR holds what Tools/companion/render_all.sh rendered. This writes:
  Resources/Companion/*.webp                      sprites shared by Mac and Windows/Linux
  Sources/Openbar/Services/Notch/NotchMascotArt.swift
  windows/src/mascot/art.ts                       the same tables for both apps

Every place is measured in shares of the companion's box, the square the
island gives it. The box is set from the bare body facing front: the body is
0.84 of the box high and stands 0.04 above its bottom, as the old ball did, so
the placement code keeps its numbers. Sprites are larger than the box: what it
wears reaches above it and its fuzz past its sides.
"""
from __future__ import annotations

import ast
import json
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(ROOT, "Resources", "Companion")
SWIFT = os.path.join(ROOT, "Sources", "Openbar", "Services", "Notch", "NotchMascotArt.swift")
TS = os.path.join(ROOT, "windows", "src", "mascot", "art.ts")

TEXTURES = os.environ.get("COMPANION_TEXTURES", "plush,knit,gummy,clay").split(",")
SPRITE_PX = 256
EYE_PX = 96
WEBP = {"quality": 90, "method": 6}


def tables():
    """COLORS, TONES and ITEMS straight from the render script, so they never drift."""
    tree = ast.parse(open(os.path.join(HERE, "blender", "render_sprites.py")).read())
    found = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            name = node.targets[0].id
            if name in ("COLORS", "TONES", "ITEMS", "YAWS"):
                found[name] = ast.literal_eval(node.value)
    return found


def alpha_bbox(img, threshold=128):
    a = img.getchannel("A").point(lambda v: 255 if v >= threshold else 0)
    return a.getbbox()


def shrink(img, px):
    """Resize with premultiplied alpha, so fuzz keeps no dark fringe."""
    return img.convert("RGBa").resize((px, px), Image.LANCZOS).convert("RGBA")


def main(src):
    t = tables()
    colors, items, yaws = t["COLORS"], t["ITEMS"], t["YAWS"]
    anchors = json.load(open(os.path.join(src, "anchors.json")))
    res = anchors["res"]
    front = yaws.index(0)

    ref = Image.open(os.path.join(src, f"body-clay-pearl-{front}.png")).convert("RGBA")
    l, top, r, bottom = alpha_bbox(ref)
    B = (bottom - top) / 0.84
    box_left = (l + r) / 2 - B / 2
    box_top = bottom + 0.04 * B - B

    def bx(x):
        return (x - box_left) / B

    def by(y):
        return (y - box_top) / B

    sprite_rect = (bx(0), by(0), res / B, res / B)
    body_bounds = (bx(l), by(top), (r - l) / B, (bottom - top) / B)
    eyes = [((bx(e["left"]["x"]), by(e["left"]["y"])), (bx(e["right"]["x"]), by(e["right"]["y"])))
            for e in anchors["eyes"]]
    fe = anchors["eyes"][front]["left"]
    eye_size = (fe["w"] / B, fe["h"] / B)

    os.makedirs(OUT, exist_ok=True)
    for name in os.listdir(OUT):
        if name.endswith(".webp"):
            os.remove(os.path.join(OUT, name))

    written = 0
    for tex in TEXTURES:
        for color in colors:
            for i in range(len(yaws)):
                img = Image.open(os.path.join(src, f"body-{tex}-{color}-{i}.png")).convert("RGBA")
                shrink(img, SPRITE_PX).save(os.path.join(OUT, f"body-{tex}-{color}-{i}.webp"), **WEBP)
                written += 1
        for item in items:
            for i in range(len(yaws)):
                img = Image.open(os.path.join(src, f"item-{tex}-{item}-{i}.png")).convert("RGBA")
                shrink(img, SPRITE_PX).save(os.path.join(OUT, f"item-{tex}-{item}-{i}.webp"), **WEBP)
                written += 1
        for tone in ("ink", "cream"):
            img = Image.open(os.path.join(src, f"eye-{tex}-{tone}.png")).convert("RGB")
            img.resize((EYE_PX, EYE_PX), Image.LANCZOS).save(os.path.join(OUT, f"eye-{tex}-{tone}.webp"), **WEBP)
            written += 1

    # Where an item covers the eyes. Its own shadow on the body only darkens
    # them, so only the solid item hides them (alpha above ~0.85).
    headroom, masked = {}, []
    for item in items:
        tops, covers = [], False
        masks = []
        for i in range(len(yaws)):
            img = Image.open(os.path.join(src, f"item-clay-{item}-{i}.png")).convert("RGBA")
            a = img.getchannel("A")
            solid = a.point(lambda v: 0 if v < 200 else min(255, (v - 200) * 255 // 40))
            bb = alpha_bbox(img, 128)
            if bb:
                tops.append(bb[1])
            # does the solid item reach into where an eye may be?
            e = anchors["eyes"][i]
            for eye in (e["left"], e["right"]):
                w, h = eye["w"] * 1.6, eye["h"] * 1.6
                region = solid.crop((int(eye["x"] - w), int(eye["y"] - h), int(eye["x"] + w), int(eye["y"] + h)))
                if region.getbbox():
                    covers = True
            window = Image.new("RGBA", img.size, (255, 255, 255, 255))
            window.putalpha(solid.point(lambda v: 255 - v))
            masks.append(window)
        headroom[item] = max(0.0, -by(min(tops))) if tops else 0.0
        if covers:
            masked.append(item)
            for i, window in enumerate(masks):
                window.resize((SPRITE_PX, SPRITE_PX), Image.LANCZOS).save(
                    os.path.join(OUT, f"mask-{item}-{i}.webp"), **WEBP)
                written += 1

    swatches = {name: rgb for name, (rgb, _tone) in colors.items()}
    cream = [name for name, (_rgb, tone) in colors.items() if tone == "cream"]
    manifest = {
        "yaws": yaws, "frontYaw": front, "spriteRect": sprite_rect, "bodyBounds": body_bounds,
        "eyeSize": eye_size, "eyes": eyes, "headroom": headroom, "maskedItems": masked,
        "swatches": swatches, "creamEyes": cream, "textures": TEXTURES, "items": ["none"] + list(items),
        "palettes": list(colors),
    }
    json.dump(manifest, open(os.path.join(OUT, "companion.json"), "w"), indent=1)
    write_swift(manifest)
    write_ts(manifest)
    size = sum(os.path.getsize(os.path.join(OUT, n)) for n in os.listdir(OUT))
    print(f"wrote {written} sprites, {size / 1e6:.1f} MB, box {B:.1f}px of {res}")


def f(v):
    return f"{v:.4f}"


def write_swift(m):
    rect = lambda r: f"CGRect(x: {f(r[0])}, y: {f(r[1])}, width: {f(r[2])}, height: {f(r[3])})"
    pt = lambda p: f"CGPoint(x: {f(p[0])}, y: {f(p[1])})"
    lines = [
        "// SPDX-License-Identifier: GPL-3.0-or-later",
        "// Copyright (C) 2026 Openbar",
        "",
        "// Generated by Tools/companion/build_assets.py from the Blender renders.",
        "// Do not edit: change the renders and run it again. windows/src/mascot/art.ts",
        "// holds the same tables for Windows and Linux.",
        "",
        "import CoreGraphics",
        "",
        "/// Where the companion's sprites and eyes sit, in shares of its box.",
        "enum NotchMascotArt {",
        "    /// The turns of its head the sprites are drawn at, in degrees.",
        f"    static let yaws: [Int] = {m['yaws']}",
        f"    static let frontYaw = {m['frontYaw']}",
        "    /// Where every sprite is drawn around the box.",
        f"    static let spriteRect = {rect(m['spriteRect'])}",
        "    /// The bare body facing front.",
        f"    static let bodyBounds = {rect(m['bodyBounds'])}",
        "    /// An open eye's width and height.",
        f"    static let eyeSize = CGSize(width: {f(m['eyeSize'][0])}, height: {f(m['eyeSize'][1])})",
        "    /// The eyes' centres for each turn of the head.",
        "    static let eyes: [(left: CGPoint, right: CGPoint)] = [",
    ]
    for left, right in m["eyes"]:
        lines.append(f"        (left: {pt(left)}, right: {pt(right)}),")
    lines.append("    ]")
    lines.append("    /// How far above the box each item reaches.")
    lines.append("    static let headroom: [NotchMascotItem: CGFloat] = [")
    for k, v in m["headroom"].items():
        lines.append(f"        .{k}: {f(v)},")
    lines.append("    ]")
    lines.append("    /// Items that cover the eyes where they turn, with a window the eyes show through.")
    lines.append("    static let maskedItems: Set<NotchMascotItem> = [" + ", ".join(f".{k}" for k in m["maskedItems"]) + "]")
    lines.append("    /// Each color's felt, in sRGB.")
    lines.append("    static let swatches: [NotchMascotPalette: (Int, Int, Int)] = [")
    for k, (r, g, b) in m["swatches"].items():
        lines.append(f"        .{k}: ({r}, {g}, {b}),")
    lines.append("    ]")
    lines.append("    static let creamEyes: Set<NotchMascotPalette> = [" + ", ".join(f".{k}" for k in m["creamEyes"]) + "]")
    lines.append("}")
    open(SWIFT, "w").write("\n".join(lines) + "\n")


def write_ts(m):
    pt = lambda p: f"{{ x: {f(p[0])}, y: {f(p[1])} }}"
    rect = lambda r: f"{{ x: {f(r[0])}, y: {f(r[1])}, width: {f(r[2])}, height: {f(r[3])} }}"
    lines = [
        "// Generated by Tools/companion/build_assets.py from the Blender renders.",
        "// Do not edit: change the renders and run it again. NotchMascotArt.swift",
        "// holds the same tables for the Mac.",
        "",
        "/** Where the companion's sprites and eyes sit, in shares of its box. */",
        "export const ART = {",
        f"  yaws: {json.dumps(m['yaws'])},",
        f"  frontYaw: {m['frontYaw']},",
        f"  spriteRect: {rect(m['spriteRect'])},",
        f"  bodyBounds: {rect(m['bodyBounds'])},",
        f"  eyeSize: {{ width: {f(m['eyeSize'][0])}, height: {f(m['eyeSize'][1])} }},",
        "  eyes: [",
    ]
    for left, right in m["eyes"]:
        lines.append(f"    {{ left: {pt(left)}, right: {pt(right)} }},")
    lines.append("  ],")
    lines.append("  headroom: {")
    for k, v in m["headroom"].items():
        lines.append(f"    {k}: {f(v)},")
    lines.append("  } as Record<string, number>,")
    lines.append(f"  maskedItems: {json.dumps(m['maskedItems'])} as string[],")
    lines.append("  swatches: {")
    for k, rgb in m["swatches"].items():
        lines.append(f"    {k}: {json.dumps(list(rgb))},")
    lines.append("  } as Record<string, [number, number, number]>,")
    lines.append(f"  creamEyes: {json.dumps(m['creamEyes'])} as string[],")
    lines.append("} as const;")
    open(TS, "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Desktop/Projekty/openbar-promo/companion/sprites"))
