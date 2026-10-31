// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The low-battery warning (NotchSupport.swift:batteryWarning): the resting
// charge turns red at or below the chosen level once the person asks for it,
// and amber a little earlier when they want a first warning. Neither shows
// while the computer is plugged in.

import type { BatteryReading } from "../system/types";
import type { IslandSettings } from "./settings";

export type BatteryWarning = "none" | "early" | "low";

export const DEFAULT_LOW_BATTERY_THRESHOLD = 10;
export const DEFAULT_EARLY_BATTERY_THRESHOLD = 20;
/** Both level sliders share this scale so their knobs line up. */
export const BATTERY_LEVEL_SCALE = { min: 1, max: 100 } as const;

/** Red stops one short of the top so amber always has room above it. */
export function lowBatteryThreshold(value: number): number {
  return Math.min(99, Math.max(1, Math.round(value)));
}

export function sanitizedEarlyThreshold(value: number): number {
  return Math.min(100, Math.max(2, Math.round(value)));
}

/** The amber level, kept at least one point above the red one so the early warning always comes first. */
export function earlyBatteryThreshold(value: number, red: number): number {
  return Math.max(sanitizedEarlyThreshold(value), lowBatteryThreshold(red) + 1);
}

type WarningSettings = Pick<
  IslandSettings,
  "lowBatteryTint" | "lowBatteryThreshold" | "lowBatteryEarly" | "lowBatteryEarlyThreshold" | "lowBatteryTray"
>;

export function batteryWarning(battery: BatteryReading | null, settings: WarningSettings): BatteryWarning {
  if (!settings.lowBatteryTint || !battery || battery.pluggedIn || battery.charging || battery.percent === null) {
    return "none";
  }
  if (battery.percent <= lowBatteryThreshold(settings.lowBatteryThreshold)) return "low";
  if (settings.lowBatteryEarly && battery.percent <= sanitizedEarlyThreshold(settings.lowBatteryEarlyThreshold)) {
    return "early";
  }
  return "none";
}

/** The tray icon follows the same warning unless the person keeps it to the island. */
export function trayBatteryWarning(battery: BatteryReading | null, settings: WarningSettings): BatteryWarning {
  return settings.lowBatteryTray ? batteryWarning(battery, settings) : "none";
}

/** Both halves of the charge turn amber, then red, together as it runs low. */
export const BATTERY_WARNING_COLORS: Record<BatteryWarning, string | undefined> = {
  none: undefined,
  early: "#ff9f0a",
  low: "#ff453a",
};
