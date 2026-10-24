// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../island/settings";
import { DEFAULT_SESSION_PREFS, loadPrefs, mergePatch, onPrefs, resolve, setPrefs } from "./prefs";

describe("preferences", () => {
  it("fill what was never set from the defaults, nested objects too", () => {
    const p = resolve({
      island: { silhouette: "capsule", capsuleFit: { width: 4 } as never, sections: { music: false } },
    });
    expect(p.island.silhouette).toBe("capsule");
    expect(p.island.capsuleFit).toEqual({ ...DEFAULT_SETTINGS.capsuleFit, width: 4 });
    expect(p.island.sections).toEqual({ ...DEFAULT_SETTINGS.sections, music: false });
    expect(p.island.layout).toBe(DEFAULT_SETTINGS.layout);
    expect(p.sessions).toEqual(DEFAULT_SESSION_PREFS);
    expect(resolve(null)).toEqual({ island: DEFAULT_SETTINGS, sessions: DEFAULT_SESSION_PREFS });
  });

  it("merge patches: objects merge, lists replace, null removes", () => {
    const stored: Record<string, unknown> = {
      island: { hiddenModules: ["camera"], outline: true },
      sessions: { live: false },
    };
    mergePatch(stored, { island: { hiddenModules: ["watch"], outline: null }, sessions: { buddies: false } });
    expect(stored).toEqual({ island: { hiddenModules: ["watch"] }, sessions: { live: false, buddies: false } });
  });

  it("fill a partly set Sessions section from its defaults", () => {
    expect(resolve({ sessions: { previews: true } }).sessions).toEqual({ ...DEFAULT_SESSION_PREFS, previews: true });
  });

  it("merge a patch into an object that was never stored, keeping siblings later", () => {
    const stored: Record<string, unknown> = {};
    mergePatch(stored, { island: { capsuleFit: { width: 4, height: 0, drop: 0 } } });
    mergePatch(stored, { island: { capsuleFit: { drop: 2 } as never, outline: true } });
    expect(stored).toEqual({ island: { capsuleFit: { width: 4, height: 0, drop: 2 }, outline: true } });
  });

  it("tell listeners about every change, outside the app too", async () => {
    const seen: boolean[] = [];
    const stop = await onPrefs((p) => seen.push(p.island.outline));
    await setPrefs({ island: { outline: true } });
    expect((await loadPrefs()).island.outline).toBe(true);
    await setPrefs({ island: { outline: null } });
    expect(seen).toEqual([true, DEFAULT_SETTINGS.outline]);
    stop();
  });
});
