// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { gridHeight, hoverInset, railCapacity, railHeight, systemRowRanges } from "./layout";

describe("system grid rows", () => {
  it("fits 128-wide cards with 10 between them", () => {
    expect(railCapacity(128, 128, 10)).toBe(1);
    expect(railCapacity(265, 128, 10)).toBe(1);
    expect(railCapacity(266, 128, 10)).toBe(2);
    expect(railCapacity(486, 128, 10)).toBe(3);
    expect(railCapacity(0, 128, 10)).toBe(1);
    expect(railCapacity(Infinity, 128, 10)).toBe(1);
  });

  it("balances rows so none is left short, earlier rows taking the remainder", () => {
    // Three columns fit (the spacious island: 504 wide, 9 inset each side).
    expect(systemRowRanges(7, 486)).toEqual([
      [0, 3],
      [3, 5],
      [5, 7],
    ]);
    expect(systemRowRanges(6, 486)).toEqual([
      [0, 3],
      [3, 6],
    ]);
    expect(systemRowRanges(5, 486)).toEqual([
      [0, 3],
      [3, 5],
    ]);
    expect(systemRowRanges(4, 486)).toEqual([
      [0, 2],
      [2, 4],
    ]);
    expect(systemRowRanges(3, 486)).toEqual([[0, 3]]);
    expect(systemRowRanges(1, 486)).toEqual([[0, 1]]);
    expect(systemRowRanges(0, 486)).toEqual([]);
  });

  it("keeps reading order across every card on a wide or narrow page", () => {
    for (const width of [100, 300, 486, 1000]) {
      for (let count = 1; count <= 9; count++) {
        const rows = systemRowRanges(count, width);
        expect(rows[0]![0]).toBe(0);
        expect(rows.at(-1)![1]).toBe(count);
        rows.slice(1).forEach(([start], i) => expect(start).toBe(rows[i]![1]));
        const sizes = rows.map(([a, b]) => b - a);
        expect(Math.max(...sizes) - Math.min(...sizes)).toBeLessThanOrEqual(1);
        expect(Math.max(...sizes)).toBeLessThanOrEqual(railCapacity(width, 128, 10));
      }
    }
    // Seven fit across 1000, so eight split four and four rather than seven and one.
    expect(systemRowRanges(8, 1000)).toEqual([
      [0, 4],
      [4, 8],
    ]);
  });

  it("measures the grid with room for the hover lift", () => {
    expect(hoverInset(504)).toBe(9);
    expect(hoverInset(424)).toBe(8);
    expect(hoverInset(-5)).toBe(1);
    expect(railHeight(2, 72, 10)).toBe(154);
    expect(railHeight(0, 72, 10)).toBe(72);
    expect(gridHeight(6, 504)).toBe(154 + 18);
    expect(gridHeight(7, 504)).toBe(236 + 18);
    expect(gridHeight(0, 504)).toBe(140);
  });
});
