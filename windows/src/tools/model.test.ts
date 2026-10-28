// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { mergedOrder, moveIndex, orderedIds, railLayout, splitTools, withHidden } from "./model";

const DEFAULTS = ["a", "b", "c", "d", "e"] as const;

describe("tool order", () => {
  it("keeps the stored order, drops unknown ids and appends new tools at the end", () => {
    expect(orderedIds(["c", "zzz", "a", "c"], DEFAULTS)).toEqual(["c", "a", "b", "d", "e"]);
    expect(orderedIds([], DEFAULTS)).toEqual([...DEFAULTS]);
  });

  it("splits available tools into the grid and the tray", () => {
    const lists = splitTools(["e", "a"], DEFAULTS, ["a", "d"], (id) => id !== "b");
    expect(lists.visible).toEqual(["e", "c"]);
    expect(lists.hidden).toEqual(["a", "d"]);
  });

  it("rearranging the grid leaves hidden tools in their slots", () => {
    // Full order a b c d e, with b hidden: the grid shows a c d e and moves e first.
    expect(mergedOrder(["a", "b", "c", "d", "e"], ["e", "a", "c", "d"])).toEqual(["e", "b", "a", "c", "d"]);
  });

  it("hides and shows tools, keeping the stored list sorted and unique", () => {
    expect(withHidden(["c"], "a", true)).toEqual(["a", "c"]);
    expect(withHidden(["a", "c"], "a", true)).toEqual(["a", "c"]);
    expect(withHidden(["a", "c"], "a", false)).toEqual(["c"]);
  });
});

describe("rail", () => {
  it("lays eight tools in one row of the spacious page", () => {
    // 504 wide: 6 per row at 76 + 6; 264 tall fits 3 rows, 2 are needed.
    const layout = railLayout(8, 504, 264);
    expect(layout.rows).toBe(2);
    expect(layout.columns).toBe(4);
    expect(layout.gap).toBe(6);
    expect(layout.titleInset).toBe(0);
  });

  it("closes its gaps before it scrolls, and the titles give up the difference", () => {
    // One row of 6 needs 6 × 76 + 5 × 6 = 486; at 470 the gap closes to 2.8.
    const layout = railLayout(6, 470, 72);
    expect(layout.rows).toBe(1);
    expect(layout.gap).toBeCloseTo(2.8);
    expect(layout.titleInset).toBeCloseTo(1.6);
  });

  it("scrolls once the columns don't fit even without gaps", () => {
    const layout = railLayout(8, 424, 72);
    expect(layout.rows).toBe(1);
    expect(layout.gap).toBeNull();
  });

  it("walks the tiles in reading order while the columns fit, clamped at the edges", () => {
    const layout = railLayout(8, 504, 264); // 2 rows × 4
    expect(moveIndex(0, 8, layout, "right")).toBe(1);
    expect(moveIndex(3, 8, layout, "right")).toBe(3);
    expect(moveIndex(1, 8, layout, "down")).toBe(5);
    expect(moveIndex(5, 8, layout, "down")).toBe(5);
    expect(moveIndex(4, 8, layout, "left")).toBe(4);
  });

  it("walks down the columns once the rail scrolls", () => {
    const layout = railLayout(9, 300, 150); // 2 rows, scrolling
    expect(layout.gap).toBeNull();
    expect(moveIndex(0, 9, layout, "down")).toBe(1);
    expect(moveIndex(0, 9, layout, "right")).toBe(2);
    expect(moveIndex(1, 9, layout, "down")).toBe(1);
  });
});
