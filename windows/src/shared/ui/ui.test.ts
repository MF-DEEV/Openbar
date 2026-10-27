// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { reorderedIds } from "./reorder";
import { parsePercent } from "./controls";
import { closeTopLayer, openLayerCount } from "../../island/pageLayer";

describe("reorderedIds", () => {
  it("moves an item before or after its target", () => {
    expect(reorderedIds(["a", "b", "c", "d"], "a", "c", false)).toEqual(["b", "a", "c", "d"]);
    expect(reorderedIds(["a", "b", "c", "d"], "a", "c", true)).toEqual(["b", "c", "a", "d"]);
    expect(reorderedIds(["a", "b", "c", "d"], "d", "a", false)).toEqual(["d", "a", "b", "c"]);
  });
  it("keeps the order when the target is itself or unknown", () => {
    expect(reorderedIds(["a", "b"], "a", "a", true)).toEqual(["a", "b"]);
    expect(reorderedIds(["a", "b"], "a", "z", true)).toEqual(["a", "b"]);
  });
});

describe("page layers", () => {
  it("reports nothing to close when no layer is open", () => {
    expect(openLayerCount()).toBe(0);
    expect(closeTopLayer()).toBe(false);
  });
});

describe("parsePercent", () => {
  it("reads typed numbers with or without a percent sign or a decimal comma", () => {
    expect(parsePercent("40")).toBe(40);
    expect(parsePercent(" 40 % ")).toBe(40);
    expect(parsePercent("40,5")).toBe(40.5);
  });
  it("leaves the value alone for an empty field or anything that isn't a number", () => {
    expect(parsePercent("")).toBeNull();
    expect(parsePercent("  ")).toBeNull();
    expect(parsePercent("%")).toBeNull();
    expect(parsePercent("abc")).toBeNull();
  });
});
