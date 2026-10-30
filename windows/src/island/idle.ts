// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { visibleModules } from "./modules";
import { type IdleContent, type IslandSettings, sectionEnabled } from "./settings";
import type { BatteryReading } from "../system/types";

/** NotchSupport.canRest: unavailable choices wait without changing the saved value. */
export function configuredIdleContent(settings: IslandSettings): IdleContent {
  const choice = settings.idleContent;
  if (choice === "battery") return settings.hiddenSystemMetrics.includes("power") ? "none" : choice;
  if (choice === "music" || choice === "agents") {
    const modules = visibleModules(settings.moduleOrder, settings.hiddenModules, (id) => sectionEnabled(settings, id));
    return modules.includes(choice) ? choice : "none";
  }
  return "none";
}

/** NotchSupport.visibleIdleContent / NotchService.idleContent. */
export function visibleIdleContent(
  settings: IslandSettings,
  battery: BatteryReading | null,
  playing: boolean,
): IdleContent {
  const choice = configuredIdleContent(settings);
  if (choice === "music" && (!playing || !settings.showPlayingMusic)) return "none";
  if (choice === "battery" && !battery) return "none";
  return choice;
}
