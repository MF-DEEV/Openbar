// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
import { expect, it } from "vitest";
import { DEFAULT_SETTINGS, type IslandSettings } from "../island/settings";
import { modulesOf } from "../island/store";
import { DEFAULT_SESSION_PREFS, type SessionsPlacement } from "../shared/prefs";
import { islandSettings, sessionsInIsland, sessionsInTray, sessionsSwitchedOn } from "./placement";

function prefs(placement: SessionsPlacement, island: Partial<IslandSettings> = {}) {
  return {
    island: { ...DEFAULT_SETTINGS, ...island },
    sessions: { ...DEFAULT_SESSION_PREFS, placement },
  };
}

it("keeps the sessions in the island until the person chooses otherwise", () => {
  expect(DEFAULT_SESSION_PREFS.placement).toBe("island");
  const p = prefs("island");
  expect(sessionsInIsland(p)).toBe(true);
  expect(sessionsInTray(p)).toBe(false);
  expect(islandSettings(p)).toBe(p.island);
});

it("shows them in the tray with the island on or off", () => {
  expect(sessionsInTray(prefs("tray"))).toBe(true);
  expect(sessionsInIsland(prefs("tray"))).toBe(false);
  expect(sessionsInTray(prefs("tray", { enabled: false }))).toBe(true);
  expect(sessionsInTray(prefs("both", { enabled: false }))).toBe(true);
  expect(sessionsInIsland(prefs("both", { enabled: false }))).toBe(false);
  expect(sessionsInIsland(prefs("both"))).toBe(true);
  expect(sessionsInIsland(prefs("island", { enabled: false }))).toBe(false);
});

it("turns them off everywhere with the section's own switch", () => {
  const hidden = prefs("both", { hiddenModules: ["sessions"] });
  expect(sessionsSwitchedOn(hidden.island)).toBe(false);
  expect(sessionsInTray(hidden)).toBe(false);
  expect(sessionsInIsland(hidden)).toBe(false);
  const off = prefs("tray", { sections: { ...DEFAULT_SETTINGS.sections, sessions: false } });
  expect(sessionsInTray(off)).toBe(false);
});

it("leaves the Sessions section out of the island when the tray alone shows them", () => {
  const tray = prefs("tray");
  const view = islandSettings(tray);
  expect(modulesOf({ settings: view })).not.toContain("sessions");
  expect(modulesOf({ settings: tray.island })).toContain("sessions");
  // Everything else stays as set, and nothing is written back.
  expect(view.enabled).toBe(tray.island.enabled);
  expect(tray.island.sections.sessions).not.toBe(false);
  expect(modulesOf({ settings: islandSettings(prefs("both")) })).toContain("sessions");
});
