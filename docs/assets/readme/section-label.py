#!/usr/bin/env python3
"""Put a centred label between two hairlines above a README logo row.

    python3 docs/assets/readme/section-label.py

Run it again after regenerating agents-*.svg or terminals-*.svg: it drops
any label it added before, counts the names in the row and draws the label
again, so "18 agents" stays true when a mark is added.
"""
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif"
COLORS = {"dark": ("#9198A1", "#3D444D"), "light": ("#59636E", "#D1D9E0")}

# file stem -> (label, space above the label)
ROWS = {
    "agents/agents": ("{n} agents", 24),
    "terminals/terminals": ("{n} terminals, editors and containers", 16),
}

LABEL_SIZE = 18
LABEL_GAP = 31  # baseline of the label to the top of the first row of marks
CHAR_WIDTH = 10.2  # rough width of one character at LABEL_SIZE, weight 600
MARGIN = 40  # hairlines stop this far from the edges


def strip(src):
    """Remove an earlier label and give the row back its own height."""
    old = re.search(r'<g id="section-body" transform="translate\(0 (\d+)\)">', src)
    if old:
        shift = int(old.group(1))
        src = re.sub(r'height="(\d+)" viewBox="0 0 (\d+) (\d+)"',
                     lambda m: f'height="{int(m.group(1)) - shift}" viewBox="0 0 {m.group(2)} {int(m.group(3)) - shift}"',
                     src, count=1)
    src = re.sub(r'<g id="section-label">.*?</g>\n<g id="section-body" transform="[^"]*">\n', "", src, flags=re.S)
    return src.replace("</g><!-- section-body -->\n", "")


def label(path, text, pad):
    theme = "dark" if path.endswith("-dark.svg") else "light"
    muted, line = COLORS[theme]
    src = strip(open(path).read())

    m = re.match(r'<svg ([^>]*?)width="(\d+)" height="(\d+)" viewBox="0 0 (\d+) (\d+)"', src)
    width, height = int(m.group(2)), int(m.group(3))
    count = len(re.findall(r"<text ", src))
    text = text.format(n=count)

    top = min(float(v) for v in re.findall(r'translate\([\d.]+ ([\d.]+)\)|<svg x="[\d.]+" y="([\d.]+)"', src)
              for v in v if v)
    baseline = pad + LABEL_SIZE
    shift = round(baseline + LABEL_GAP - top)
    mid = width / 2
    half = len(text) * CHAR_WIDTH / 2 + 16
    rule_y = baseline - 6

    head = (
        f'<g id="section-label">\n'
        f'  <line x1="{MARGIN}" y1="{rule_y}" x2="{mid - half:.1f}" y2="{rule_y}" stroke="{line}"/>\n'
        f'  <text x="{mid:.0f}" y="{baseline}" text-anchor="middle" fill="{muted}" font-family="{FONT}" '
        f'font-size="{LABEL_SIZE}" font-weight="600">{text}</text>\n'
        f'  <line x1="{mid + half:.1f}" y1="{rule_y}" x2="{width - MARGIN}" y2="{rule_y}" stroke="{line}"/>\n'
        f'</g>\n<g id="section-body" transform="translate(0 {shift})">\n'
    )
    new_h = height + shift
    src = src.replace(m.group(0), f'<svg {m.group(1)}width="{width}" height="{new_h}" viewBox="0 0 {width} {new_h}"', 1)
    title_end = src.index("</title>\n") + len("</title>\n")
    src = src[:title_end] + head + src[title_end:]
    src = src[: src.rindex("</svg>")] + "</g><!-- section-body -->\n</svg>\n"
    open(path, "w").write(src)
    print(f"{os.path.relpath(path, HERE)}: {text!r}, {width}x{new_h}")


for stem, (text, pad) in ROWS.items():
    for theme in COLORS:
        label(os.path.join(HERE, f"{stem}-{theme}.svg"), text, pad)
