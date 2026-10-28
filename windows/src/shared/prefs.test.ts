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

describe("controls and mixer prefs", () => {
  it("keeps known home-page items and a well-formed arrangement", () => {
    const prefs = resolve({
      island: {
        controlOrder: ["timer", "nope", "timer", 3] as never,
        hiddenControls: ["mixer"],
        mixerAppArrangement: { order: ["c:\\a.exe", 1, ""], pinned: "x" } as never,
        mixerHiddenApps: { "c:\\b.exe": "B", bad: 2 } as never,
      },
    });
    expect(prefs.island.controlOrder).toEqual(["timer"]);
    expect(prefs.island.hiddenControls).toEqual(["mixer"]);
    expect(prefs.island.mixerAppArrangement).toEqual({ order: ["c:\\a.exe"], pinned: [] });
    expect(prefs.island.mixerHiddenApps).toEqual({ "c:\\b.exe": "B" });
  });

  it("falls back to the Mac's defaults", () => {
    const prefs = resolve({ island: { mixerAppArrangement: [] as never } });
    expect(prefs.island.hiddenControls).toContain("microphone");
    expect(prefs.island.mixerAppArrangement).toEqual({ order: [], pinned: [] });
    expect(prefs.island.brightnessControlEnabled).toBe(false);
    expect(prefs.island.keepAwakeActivity).toBe(false);
  });

  it("keeps the Mixer options within what the backend reads", () => {
    const prefs = resolve({
      island: {
        mixerHeadphonesDisconnectVolumePercent: 140.4,
        soundOutputSwitcherShortcut: "  ",
        soundOutputSwitcherDevices: ["a", "", "a", 3, " ", "b"] as never,
      },
    });
    expect(prefs.island.mixerHeadphonesDisconnectVolumePercent).toBe(100);
    expect(prefs.island.soundOutputSwitcherShortcut).toBe(DEFAULT_SETTINGS.soundOutputSwitcherShortcut);
    expect(prefs.island.soundOutputSwitcherDevices).toEqual(["a", "b"]);
    expect(
      resolve({ island: { mixerHeadphonesDisconnectVolumePercent: -3 } }).island.mixerHeadphonesDisconnectVolumePercent,
    ).toBe(0);
    expect(
      resolve({ island: { mixerHeadphonesDisconnectVolumePercent: 32.6 } }).island
        .mixerHeadphonesDisconnectVolumePercent,
    ).toBe(33);
    expect(
      resolve({ island: { mixerHeadphonesDisconnectVolumePercent: "x" as never } }).island
        .mixerHeadphonesDisconnectVolumePercent,
    ).toBe(DEFAULT_SETTINGS.mixerHeadphonesDisconnectVolumePercent);
  });

  it("starts the Mixer options off, as the Mac does", () => {
    const { island } = resolve(null);
    expect(island.mixerLowerVolumeOnHeadphonesDisconnect).toBe(false);
    expect(island.mixerHeadphonesDisconnectVolumePercent).toBe(25);
    expect(island.preciseVolumeRollerEnabled).toBe(false);
    expect(island.soundOutputSwitcherEnabled).toBe(false);
    expect(island.soundOutputSwitcherShortcut).toBe("Ctrl+Alt+Shift+KeyS");
    expect(island.soundOutputSwitcherDevices).toEqual([]);
  });
});
