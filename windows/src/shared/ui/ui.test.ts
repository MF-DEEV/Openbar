// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { reorderedIds } from "./reorder";
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
