// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import type { BatteryReading } from "../system/types";

export const useBattery = create<{ battery: BatteryReading | null }>(() => ({ battery: null }));

/** Listen before reading so a slow initial response can't overwrite a newer event. */
export async function subscribeBattery(): Promise<() => void> {
  let version = 0;
  const off = await bridge.power.onBattery((battery) => {
    version++;
    useBattery.setState({ battery });
  });
  const at = version;
  try {
    const battery = await bridge.power.battery();
    if (version === at) useBattery.setState({ battery: battery ?? null });
  } catch {
    // Unsupported systems and failed readings rest empty until a new event.
  }
  return off;
}
