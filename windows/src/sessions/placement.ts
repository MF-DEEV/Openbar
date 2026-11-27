// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Where the sessions show (#331): in the island, in a panel from their own
// tray icon, or in both. The tray keeps them with the island turned off.
// The same as the Mac's NotchClaudeSessionsSupport placement.

import { create } from "zustand";
import { sectionEnabled, type IslandSettings } from "../island/settings";
import type { Prefs, SessionsPlacement } from "../shared/prefs";

/** The Sessions section's own switch: on, and not hidden from the island's list, where it is turned off. */
export function sessionsSwitchedOn(island: Pick<IslandSettings, "hiddenModules" | "sections">): boolean {
  return !island.hiddenModules.includes("sessions") && sectionEnabled(island as IslandSettings, "sessions");
}

/** The island shows them: its page, the strip beside the camera and its notices. */
export function sessionsInIsland(prefs: Pick<Prefs, "island" | "sessions">): boolean {
  return prefs.island.enabled && sessionsSwitchedOn(prefs.island) && prefs.sessions.placement !== "tray";
}

/** Their tray icon shows them, with or without the island. */
export function sessionsInTray(prefs: Pick<Prefs, "island" | "sessions">): boolean {
  return sessionsSwitchedOn(prefs.island) && prefs.sessions.placement !== "island";
}

/**
 * The island's settings as the island reads them. With the sessions in the
 * tray alone the island has no Sessions section, so no page, strip, notice
 * or gesture brings them up there; everything else stays as set.
 */
export function islandSettings(prefs: Pick<Prefs, "island" | "sessions">): IslandSettings {
  if (prefs.sessions.placement !== "tray" || prefs.island.sections.sessions === false) return prefs.island;
  return { ...prefs.island, sections: { ...prefs.island.sections, sessions: false } };
}

interface PlacementState {
  placement: SessionsPlacement;
  inIsland: boolean;
  inTray: boolean;
}

/** Where the sessions show now, read from the stored settings (not the island's view of them). */
export const usePlacement = create<PlacementState>()(() => ({ placement: "island", inIsland: true, inTray: false }));

export function applyPlacement(prefs: Pick<Prefs, "island" | "sessions">) {
  const next = {
    placement: prefs.sessions.placement,
    inIsland: sessionsInIsland(prefs),
    inTray: sessionsInTray(prefs),
  };
  const now = usePlacement.getState();
  if (now.placement !== next.placement || now.inIsland !== next.inIsland || now.inTray !== next.inTray)
    usePlacement.setState(next);
}

/** Sessions are followed somewhere: their sounds, reminders and asks go on. */
export const sessionsFollowed = () => {
  const { inIsland, inTray } = usePlacement.getState();
  return inIsland || inTray;
};
