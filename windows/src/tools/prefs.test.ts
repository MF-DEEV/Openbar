// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { resolve } from "../shared/prefs";

describe("tools and scratchpad preferences", () => {
  it("keep known choices and read unknown ones as the default", () => {
    const ok = resolve({ island: { colorPickerFormat: "hsl", scratchpadRetention: "week" } }).island;
    expect([ok.colorPickerFormat, ok.scratchpadRetention]).toEqual(["hsl", "week"]);
    const bad = resolve({ island: { colorPickerFormat: "swiftui", scratchpadRetention: "year" } as never }).island;
    expect([bad.colorPickerFormat, bad.scratchpadRetention]).toEqual(["hex", "never"]);
  });

  it("take lists of names once each, without blanks or non-strings", () => {
    const island = resolve({
      island: { toolsHidden: ["screenshot", "", "screenshot", 3, "  "], ejectExcluded: "E:" } as never,
    }).island;
    expect(island.toolsHidden).toEqual(["screenshot"]);
    expect(island.ejectExcluded).toEqual([]);
    expect(island.togglesOrder).toEqual([]);
  });
});
