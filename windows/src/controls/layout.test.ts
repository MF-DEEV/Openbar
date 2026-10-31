// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { MODULES, type ModuleId } from "../island/modules";
import {
  controlsLayout,
  controlsOf,
  editableShortcuts,
  layoutHeight,
  levelsFitBand,
  railCapacity,
  railGap,
  railRows,
} from "./layout";
import { DEFAULT_HIDDEN_CONTROLS } from "./types";

const ALL: ModuleId[] = MODULES.map((m) => m.id);

describe("home page items", () => {
  it("shows the Mac's default set: levels, the card and four tiles", () => {
    expect(controlsOf([], DEFAULT_HIDDEN_CONTROLS, ALL)).toEqual([
      "volume",
      "brightness",
      "music",
      "mixer",
      "keepAwake",
      "timer",
      "calendar",
    ]);
  });

  it("puts the stored order first and the rest after, in declaration order", () => {
    expect(controlsOf(["calendar", "keepAwake"], DEFAULT_HIDDEN_CONTROLS, ALL)).toEqual([
      "calendar",
      "keepAwake",
      "volume",
      "brightness",
      "music",
      "mixer",
      "timer",
    ]);
  });

  it("never shows what Windows lacks, even when not hidden", () => {
    const items = controlsOf([], [], ALL);
    for (const item of ["keyboardLight", "panel", "commandBar"] as const) expect(items).not.toContain(item);
    expect(items).toContain("speedTest");
    expect(items).toContain("screenshot");
  });

  it("drops a tile whose section is hidden", () => {
    const items = controlsOf([], [], ["controls", "music"]);
    expect(items).not.toContain("mixer");
    expect(items).not.toContain("speedTest");
    expect(items).not.toContain("timer");
    expect(items).toContain("music");
    expect(items).toContain("keepAwake");
  });

  it("lists every Windows tile in the editor, in order, without levels or the card", () => {
    expect(editableShortcuts(["timer"], ["controls", "mixer"])).toEqual([
      "mixer",
      "keepAwake",
      "microphone",
      "screenshot",
      "recording",
    ]);
    expect(editableShortcuts(["scratchpad"], ALL)).toEqual([
      "scratchpad",
      "mixer",
      "keepAwake",
      "timer",
      "calendar",
      "microphone",
      "screenshot",
      "recording",
      "speedTest",
    ]);
  });
});

describe("home page layout", () => {
  it("keeps the full card and one row of four tiles in a preset", () => {
    const layout = controlsLayout({ hasCard: true, levelRow: false, shortcuts: 4, width: 344, height: 264 });
    expect(layout).toMatchObject({ cardRow: 80, shortcutRows: 1, levelRow: 0 });
    expect(layoutHeight(layout)).toBe(80 + 10 + 74);
  });

  it("wraps more tiles into rows the height allows", () => {
    const layout = controlsLayout({ hasCard: true, levelRow: false, shortcuts: 7, width: 344, height: 264 });
    expect(layout.shortcutRows).toBe(2);
    expect(layoutHeight(layout)).toBe(80 + 10 + 74 * 2 + 8);
  });

  it("shortens the card before dropping a row", () => {
    const layout = controlsLayout({ hasCard: true, levelRow: false, shortcuts: 7, width: 344, height: 236 });
    expect(layout.shortcutRows).toBe(2);
    expect(layout.cardRow).toBe(70);
  });

  it("adds the level row's height when the band can't hold the levels", () => {
    const layout = controlsLayout({ hasCard: false, levelRow: true, shortcuts: 4, width: 344, height: 180 });
    expect(layout.levelRow).toBe(28);
    expect(layoutHeight(layout)).toBe(28 + 10 + 74);
  });

  it("closes the rows of a rail that would scroll to gain one", () => {
    // 9 tiles at 160 wide scroll in one row; 154 tall is 6 short of a second 74 row at 8.
    const layout = controlsLayout({ hasCard: false, levelRow: false, shortcuts: 9, width: 160, height: 154 });
    expect(layout.shortcutRows).toBe(2);
    expect(layout.shortcutRowSpacing).toBe(6);
  });

  it("holds the levels in the band all or none", () => {
    expect(levelsFitBand(2, 344)).toBe(true);
    expect(levelsFitBand(2, 175)).toBe(false);
    expect(levelsFitBand(2, 176)).toBe(true);
    expect(levelsFitBand(0, 0)).toBe(true);
  });
});

describe("rails", () => {
  it("count what fits, and close gaps down to the least before scrolling", () => {
    expect(railCapacity(344, 76, 8)).toBe(4);
    expect(railRows(7, 4, 74, 8, 156)).toBe(2);
    expect(railRows(7, 4, 74, 8, 100)).toBe(1);
    expect(railGap(4, 76, 8, 0, 344)).toBe(8);
    expect(railGap(4, 76, 8, 0, 310)).toBe(2);
    expect(railGap(5, 76, 8, 0, 310)).toBeNull();
  });
});
