// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Edge cases of stored preferences: settings.json may be edited by hand or
// come from an older build, so whatever it holds must read as something the
// windows can draw.

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../island/settings";
import { visibleModules } from "../island/modules";
import { mergePatch, resolve, type StoredPrefs } from "./prefs";

const stored = (island: Record<string, unknown>) => ({ island }) as unknown as StoredPrefs;

describe("stored preferences of the wrong shape", () => {
  it("read a non-list hiddenModules or moduleOrder as the default", () => {
    const p = resolve(stored({ hiddenModules: 5, moduleOrder: null }));
    expect(p.island.hiddenModules).toEqual(DEFAULT_SETTINGS.hiddenModules);
    expect(visibleModules(p.island.moduleOrder, [], () => true)).toHaveLength(17);
  });

  it("read unknown choices and wrong types as the default", () => {
    const p = resolve(stored({ opening: "bogus", display: "all", silhouette: 3, hoverDelay: "fast", outline: "yes" }));
    expect(p.island.opening).toBe("click");
    expect(p.island.display).toBe("primary");
    expect(p.island.silhouette).toBe(DEFAULT_SETTINGS.silhouette);
    expect(p.island.hoverDelay).toBe(DEFAULT_SETTINGS.hoverDelay);
    expect(p.island.outline).toBe(DEFAULT_SETTINGS.outline);
  });

  it("keep known section ids once, in order", () => {
    const p = resolve(stored({ moduleOrder: ["music", "nope", "music", "timer"], hiddenModules: ["camera", 7] }));
    expect(p.island.moduleOrder).toEqual(["music", "timer"]);
    expect(p.island.hiddenModules).toEqual(["camera"]);
    expect(visibleModules(["music", "music"], [], () => true).filter((id) => id === "music")).toHaveLength(1);
  });

  it("drop a null inside a patch for an object that was never stored", () => {
    const into: Record<string, unknown> = {};
    mergePatch(into, { island: { hiddenModules: null, outline: true } });
    expect(into).toEqual({ island: { outline: true } });
    expect(resolve(into as StoredPrefs).island.hiddenModules).toEqual([]);
  });
});
