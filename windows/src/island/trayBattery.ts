// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Mac tints its menu bar reading; here the tray icon wears the warning,
// whatever the island rests with.

import { bridge } from "../shared/bridge";
import { useBattery } from "./battery";
import { trayBatteryWarning } from "./lowBattery";
import { useIsland } from "./store";

export function followTrayBattery(): () => void {
  let told: string | null = null;
  const update = () => {
    const { battery } = useBattery.getState();
    const warning = trayBatteryWarning(battery, useIsland.getState().settings);
    const percent = warning === "none" ? null : (battery?.percent ?? null);
    const next = `${warning}:${percent}`;
    if (next === told) return;
    told = next;
    bridge.tray.batteryWarning(warning, percent).catch(() => {});
  };
  update();
  const offBattery = useBattery.subscribe(update);
  const offIsland = useIsland.subscribe(update);
  return () => {
    offBattery();
    offIsland();
  };
}
