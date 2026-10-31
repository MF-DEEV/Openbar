// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { resolve } from "../shared/prefs";
import type { BatteryReading } from "../system/types";
import { useBattery } from "./battery";
import { batteryWarning, earlyBatteryThreshold, lowBatteryThreshold, trayBatteryWarning } from "./lowBattery";
import { DEFAULT_SETTINGS } from "./settings";
import { useIsland } from "./store";
import { followTrayBattery } from "./trayBattery";

const on = (percent: number | null, patch: Partial<BatteryReading> = {}): BatteryReading => ({
  percent,
  charging: false,
  pluggedIn: false,
  rateWatts: null,
  secondsLeft: null,
  healthPercent: null,
  cycleCount: null,
  ...patch,
});
const tinted = { ...DEFAULT_SETTINGS, lowBatteryTint: true };

describe("battery warning", () => {
  it("is off by default, with the Mac's levels", () => {
    expect(DEFAULT_SETTINGS.lowBatteryTint).toBe(false);
    expect(DEFAULT_SETTINGS.lowBatteryThreshold).toBe(10);
    expect(DEFAULT_SETTINGS.lowBatteryEarly).toBe(false);
    expect(DEFAULT_SETTINGS.lowBatteryEarlyThreshold).toBe(20);
    expect(DEFAULT_SETTINGS.lowBatteryTray).toBe(true);
    expect(batteryWarning(on(1), DEFAULT_SETTINGS)).toBe("none");
  });

  it("turns red at or below the level, on battery only", () => {
    expect(batteryWarning(on(11), tinted)).toBe("none");
    expect(batteryWarning(on(10), tinted)).toBe("low");
    expect(batteryWarning(on(0), tinted)).toBe("low");
    expect(batteryWarning(on(5, { pluggedIn: true }), tinted)).toBe("none");
    expect(batteryWarning(on(5, { charging: true }), tinted)).toBe("none");
    expect(batteryWarning(on(null), tinted)).toBe("none");
    expect(batteryWarning(null, tinted)).toBe("none");
  });

  it("turns amber first when asked, and red still wins", () => {
    const early = { ...tinted, lowBatteryEarly: true };
    expect(batteryWarning(on(21), early)).toBe("none");
    expect(batteryWarning(on(20), early)).toBe("early");
    expect(batteryWarning(on(11), early)).toBe("early");
    expect(batteryWarning(on(10), early)).toBe("low");
    expect(batteryWarning(on(20), tinted)).toBe("none");
  });

  it("keeps the levels in range with amber above red", () => {
    expect(lowBatteryThreshold(0)).toBe(1);
    expect(lowBatteryThreshold(100)).toBe(99);
    expect(earlyBatteryThreshold(1, 1)).toBe(2);
    expect(earlyBatteryThreshold(20, 50)).toBe(51);
    expect(earlyBatteryThreshold(100, 99)).toBe(100);
    expect(batteryWarning(on(50), { ...tinted, lowBatteryThreshold: 500 })).toBe("low");
  });

  it("reaches the tray unless kept to the island", () => {
    expect(trayBatteryWarning(on(5), tinted)).toBe("low");
    expect(trayBatteryWarning(on(5), { ...tinted, lowBatteryTray: false })).toBe("none");
  });

  it("reads saved levels and drops wrong types", () => {
    const { island } = resolve({
      island: { lowBatteryTint: true, lowBatteryThreshold: 15, lowBatteryEarly: "yes" as never },
    });
    expect(island.lowBatteryTint).toBe(true);
    expect(island.lowBatteryThreshold).toBe(15);
    expect(island.lowBatteryEarly).toBe(false);
  });
});

describe("tray warning", () => {
  it("tells the tray only when the warning or the charge shown changes", () => {
    const told = vi.spyOn(bridge.tray, "batteryWarning").mockResolvedValue(undefined);
    useIsland.setState({ settings: tinted });
    useBattery.setState({ battery: on(50) });
    const stop = followTrayBattery();
    expect(told).toHaveBeenCalledTimes(1);
    expect(told).toHaveBeenLastCalledWith("none", null);
    useBattery.setState({ battery: on(49) });
    expect(told).toHaveBeenCalledTimes(1);
    useBattery.setState({ battery: on(9) });
    expect(told).toHaveBeenLastCalledWith("low", 9);
    useIsland.setState({ settings: { ...tinted, lowBatteryTray: false } });
    expect(told).toHaveBeenLastCalledWith("none", null);
    stop();
    useBattery.setState({ battery: on(8) });
    expect(told).toHaveBeenCalledTimes(3);
  });
});
