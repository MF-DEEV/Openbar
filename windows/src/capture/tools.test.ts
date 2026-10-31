// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { offersRepeat, switchesTools, toolForKey } from "./tools";

describe("capture tools", () => {
  it("keeps the Mac's number keys", () => {
    expect(toolForKey("1")).toBe("screenshot");
    expect(toolForKey("3")).toBe("text");
    expect(toolForKey("4")).toBe("color");
    expect(toolForKey("5")).toBeNull();
    expect(toolForKey("r")).toBeNull();
  });

  it("watch does not switch", () => {
    expect(switchesTools("watch")).toBe(false);
    expect(switchesTools("screenshot")).toBe(true);
  });

  it("offers R only with a last area and a tool that takes one", () => {
    const area = { x: 1, y: 1, width: 2, height: 2 };
    expect(offersRepeat("screenshot", area)).toBe(true);
    expect(offersRepeat("text", area)).toBe(true);
    expect(offersRepeat("color", area)).toBe(false);
    expect(offersRepeat("screenshot", null)).toBe(false);
    expect(offersRepeat("screenshot", undefined)).toBe(false);
  });
});
