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
  it("mirrors only the button positions below the top panel for a bottom panel", () => {
    const config = buttons(2, 2, 1);
    const size = { width: 450, height: 300 };
    const top = placeButtons(config, size);
    const bottom = placeButtons(config, size, undefined, "bottom");
    expect(bottom.map((item) => item.button.id)).toEqual(top.map((item) => item.button.id));
    for (const side of ["left", "right"] as const) {
      const upper = top.filter((item) => item.button.side === side);
      const lower = bottom.filter((item) => item.button.side === side);
      expect(lower.map((item) => item.cx)).toEqual(upper.map((item) => item.cx));
      expect(lower.map((item) => item.cy)).toEqual(upper.map((item) => size.height - item.cy).reverse());
    }
    expect(bottom.at(-1)).toMatchObject({ cx: size.width / 2, cy: -QUICK_LAYOUT.offset });
  });
  it("places the five default side controls in two rows and one interior button", () => {
    const config = buttons(2, 2, 1);
    for (const edge of ["left", "right"] as const) {
      const placed = placeButtons(config, { width: 340, height: 430 }, undefined, edge, { top: 80, bottom: 80 });
      expect(placed).toHaveLength(5);
      expect(placed.slice(0, 2).map((item) => item.cy)).toEqual([-QUICK_LAYOUT.offset, -QUICK_LAYOUT.offset]);
      expect(placed.slice(2, 4).map((item) => item.cy)).toEqual([430 + QUICK_LAYOUT.offset, 430 + QUICK_LAYOUT.offset]);
      expect(placed[4]!.cx).toBe(edge === "left" ? 340 + QUICK_LAYOUT.offset : -QUICK_LAYOUT.offset);
      expect(placed[4]!.cy).toBe(215);
      const corner = placeButtons(config, { width: 340, height: 430 }, undefined, edge, { top: 12, bottom: 80 });
      expect(corner.every((item) => item.cy >= QUICK_LAYOUT.diameter / 2)).toBe(true);
    }
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
