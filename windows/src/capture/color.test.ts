// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { formatColor } from "./color";

// The same cases as capture/color.rs: the readout must match what a click copies.
describe("colour formats", () => {
  it("writes hex with or without the hash", () => {
    expect(formatColor(255, 136, 0, "hex", false)).toBe("#FF8800");
    expect(formatColor(255, 136, 0, "hex", true)).toBe("FF8800");
  });

  it("writes rgb and hsl as the Mac does", () => {
    expect(formatColor(10, 20, 30, "rgb", true)).toBe("rgb(10, 20, 30)");
    expect(formatColor(255, 0, 128, "hsl", false)).toBe("hsl(330, 100%, 50%)");
    expect(formatColor(51, 102, 153, "hsl", false)).toBe("hsl(210, 50%, 40%)");
    expect(formatColor(128, 128, 128, "hsl", false)).toBe("hsl(0, 0%, 50%)");
  });
});
