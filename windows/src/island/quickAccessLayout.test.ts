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
