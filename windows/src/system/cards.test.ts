// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { batteryLow, diskLow, isBusy, powerReading, systemCards } from "./cards";
import { demoSnapshot } from "./demo";
import type { BatteryReading, SystemSnapshot } from "./types";

const base = (patch: Partial<SystemSnapshot> = {}): SystemSnapshot => ({
  ...demoSnapshot(1),
  cpu: 0.9,
  memoryUsed: 0.63 * 16 * 1024 ** 3,
  ...patch,
});
const battery = (patch: Partial<BatteryReading>): BatteryReading => ({
  percent: 50,
  charging: false,
  pluggedIn: false,
  rateWatts: null,
  secondsLeft: null,
  healthPercent: null,
  cycleCount: null,
  ...patch,
});
const options = { hidden: [], inBits: false };

describe("attention", () => {
  it("marks CPU, GPU and memory at 85 % or more", () => {
    expect(isBusy(0.85)).toBe(true);
    expect(isBusy(0.849)).toBe(false);
    expect(isBusy(null)).toBe(false);
    expect(isBusy(NaN)).toBe(false);
  });

  it("marks a battery at 20 % or less only while unplugged", () => {
    expect(batteryLow(battery({ percent: 20 }))).toBe(true);
    expect(batteryLow(battery({ percent: 21 }))).toBe(false);
    expect(batteryLow(battery({ percent: 5, pluggedIn: true }))).toBe(false);
    expect(batteryLow(battery({ percent: null }))).toBe(false);
  });

  it("marks a disk with less than 10 % free", () => {
    expect(diskLow(9, 100)).toBe(true);
    expect(diskLow(10, 100)).toBe(false);
    expect(diskLow(0, 0)).toBe(false);
  });
});

describe("system cards", () => {
  it("lists the Mac's cards in order with their meters and lines", () => {
    const cards = systemCards(base(), options);
    expect(cards.map((c) => c.kind)).toEqual([
      "cpu",
      "gpu",
      "memory",
      "battery",
      "network",
      "disk",
      "power",
      "connectedDevices",
    ]);
    const byKind = Object.fromEntries(cards.map((c) => [c.kind, c]));
    expect(byKind.cpu!.attention).toBe(true);
    expect(byKind.memory!.value).toBe("63%");
    expect(byKind.battery!.attention).toBe(true);
    expect(byKind.disk!.attention).toBe(true);
    expect(byKind.disk!.value).toBe("41 GB");
    expect(byKind.network!.value).toMatch(/^↓ /);
    expect(byKind.network!.detail).toMatch(/^↑ /);
    expect(byKind.connectedDevices!.value).toBe("3");
  });

  it("shows … for readings not taken yet, and nothing before the first snapshot", () => {
    const cards = systemCards(
      base({ cpu: null, netDown: null, netUp: null, memoryUsed: null, devices: null, disk: null }),
      options,
    );
    const byKind = Object.fromEntries(cards.map((c) => [c.kind, c]));
    expect(byKind.cpu!.value).toBeNull();
    expect(byKind.cpu!.level).toBeNull();
    expect(byKind.network!.value).toBeNull();
    expect(byKind.network!.detail).toBeNull();
    expect(byKind.memory!.value).toBeNull();
    expect(byKind.connectedDevices!.value).toBeNull();
    expect(byKind.disk!.attention).toBe(false);
    expect(systemCards(null, options)).toEqual([]);
  });

  it("hides cards the machine can't read or the person switched off", () => {
    const desktop = base({
      battery: null,
      supported: { cpu: true, gpu: false, memory: true, network: true, disk: true, devices: true },
    });
    expect(systemCards(desktop, options).map((c) => c.kind)).toEqual([
      "cpu",
      "memory",
      "network",
      "disk",
      "connectedDevices",
    ]);
    const kinds = systemCards(base(), { hidden: ["power", "network", "cpu"], inBits: false }).map((c) => c.kind);
    expect(kinds).toEqual(["gpu", "memory", "disk", "connectedDevices"]);
    const none = base({
      battery: null,
      supported: { cpu: false, gpu: false, memory: false, network: false, disk: false, devices: false },
    });
    expect(systemCards(none, options)).toEqual([]);
  });

  it("reads the network in the chosen unit", () => {
    const card = systemCards(base({ netDown: 1_500_000, netUp: 125 }), { hidden: [], inBits: true }).find(
      (c) => c.kind === "network",
    )!;
    expect(card.value).toBe("↓ 12 Mbps");
    expect(card.detail).toBe("↑ 1.0 Kbps");
  });

  it("shows power while the battery reports a rate, without blinking at zero", () => {
    expect(powerReading(battery({ rateWatts: -11.6 }))).toEqual({ watts: 11.6, pluggedIn: false });
    expect(powerReading(battery({ rateWatts: 25, pluggedIn: true, charging: true }))).toEqual({
      watts: 25,
      pluggedIn: true,
    });
    expect(powerReading(battery({ rateWatts: 0, pluggedIn: true }))).toEqual({ watts: 0, pluggedIn: true });
    expect(powerReading(battery({ rateWatts: null }))).toBeNull();
    expect(powerReading(null)).toBeNull();
    const charging = systemCards(
      base({ battery: battery({ rateWatts: 25, pluggedIn: true, charging: true, percent: 50 }) }),
      options,
    );
    const power = charging.find((c) => c.kind === "power")!;
    expect(power.value).toBe("25W");
    expect(power.detail).toBe("Charging");
    expect(charging.find((c) => c.kind === "battery")!.charging).toBe(true);
    const full = systemCards(base({ battery: battery({ rateWatts: 0, pluggedIn: true }) }), options);
    expect(full.find((c) => c.kind === "power")).toMatchObject({ value: "0W", detail: "Plugged in" });
    const desktop = systemCards(base({ battery: battery({ rateWatts: null }) }), options);
    expect(desktop.some((c) => c.kind === "power")).toBe(false);
  });
});
