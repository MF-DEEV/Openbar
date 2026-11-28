// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { describe, expect, it } from "vitest";
import { makeGeometry } from "./geometry";
import { freeQuickSide, placeButtons, QUICK_LAYOUT, QUICK_SIDES } from "./quickAccessLayout";
import type { QuickButton } from "./settings";

const buttons = (left: number, right: number, bottom: number): QuickButton[] =>
  QUICK_SIDES.flatMap((side, i) =>
    Array.from({ length: [left, right, bottom][i]! }, (_, n) => ({
      id: `${side}-${n}`,
      side,
      action: { kind: "pin" as const },
    })),
  );

describe("editable quick access layout", () => {
  it("turns the top layout onto the bottom and side edges with the panel", () => {
    const config = buttons(2, 2, 1);
    const size = { width: 450, height: 300 };
    const top = placeButtons(config, size);
    const bottom = placeButtons(config, size, undefined, "bottom");
    expect(bottom.map((item) => item.button.id)).toEqual(top.map((item) => item.button.id));
    expect(bottom.map((item) => [item.cx, item.cy])).toEqual(top.map((item) => [item.cx, size.height - item.cy]));
    expect(bottom.at(-1)).toMatchObject({ cx: size.width / 2, cy: -QUICK_LAYOUT.offset });
    // On a side the panel's along-edge length is its height: the top layout of a 300 × 450 panel, turned.
    const side = { width: 450, height: 300 };
    const upright = placeButtons(config, { width: side.height, height: side.width });
    const left = placeButtons(config, side, undefined, "left");
    const right = placeButtons(config, side, undefined, "right");
    expect(left.map((item) => [item.cx, item.cy])).toEqual(upright.map((item) => [item.cy, item.cx]));
    expect(right.map((item) => [item.cx, item.cy])).toEqual(upright.map((item) => [side.width - item.cy, item.cx]));
    // Left and right columns sit off the panel's ends near the screen edge; the bottom row faces the screen.
    for (const item of left.filter((p) => p.button.side !== "bottom")) {
      expect(item.cy < 0 || item.cy > side.height).toBe(true);
      expect(item.cx).toBeLessThan(side.width / 2);
    }
    expect(left.at(-1)).toMatchObject({ cx: side.width + QUICK_LAYOUT.offset, cy: side.height / 2 });
    expect(right.at(-1)).toMatchObject({ cx: -QUICK_LAYOUT.offset, cy: side.height / 2 });
  });
  it("places all circles without overlap for every supported arrangement", () => {
    for (const silhouette of ["notch", "capsule"] as const) {
      const geometry = makeGeometry({
        screen: { width: 1920, height: 1080 },
        devicePixelRatio: 1,
        silhouette,
        layout: "spacious",
      });
      for (const width of [200, 360, 600])
        for (const height of [120, 170, 640]) {
          for (let left = 0; left <= 3; left++)
            for (let right = 0; right <= 3; right++)
              for (let bottom = 0; bottom <= 3; bottom++) {
                const config = buttons(left, right, bottom);
                const placed = placeButtons(config, { width, height }, geometry);
                expect(placed).toHaveLength(config.length);
                for (let i = 0; i < placed.length; i++) {
                  expect(placed[i]!.cy).toBeGreaterThanOrEqual(QUICK_LAYOUT.diameter / 2);
                  for (let j = i + 1; j < placed.length; j++)
                    expect(
                      Math.hypot(placed[i]!.cx - placed[j]!.cx, placed[i]!.cy - placed[j]!.cy),
                    ).toBeGreaterThanOrEqual(QUICK_LAYOUT.diameter + 8);
                }
                const bottomGroup = placed.filter((p) => p.button.side === "bottom");
                if (bottomGroup.length) expect((bottomGroup[0]!.cx + bottomGroup.at(-1)!.cx) / 2).toBe(width / 2);
              }
        }
    }
  });
  it("chooses available space and stops when all nine positions are occupied", () => {
    expect(freeQuickSide(buttons(2, 2, 1))).toBe("left");
    expect(freeQuickSide(buttons(3, 2, 1))).toBe("right");
    expect(freeQuickSide(buttons(3, 3, 1))).toBe("bottom");
    expect(freeQuickSide(buttons(3, 3, 3))).toBeUndefined();
    expect(freeQuickSide(buttons(1, 1, 1), "bottom")).toBe("bottom");
  });
});
