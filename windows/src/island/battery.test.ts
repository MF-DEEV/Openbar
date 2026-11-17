// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, expect, it, vi } from "vitest";
import type { BatteryReading } from "../system/types";
import { bridge } from "../shared/bridge";
import { subscribeBattery, useBattery } from "./battery";

afterEach(() => {
  vi.restoreAllMocks();
  useBattery.setState({ battery: null });
});
const battery: BatteryReading = {
  percent: 83,
  charging: true,
  pluggedIn: true,
  rateWatts: null,
  secondsLeft: null,
  healthPercent: null,
  cycleCount: null,
};

it("listens before reading and keeps an event newer than the initial response", async () => {
  let receive!: (battery: BatteryReading | null) => void;
  let finish!: (battery: BatteryReading | null) => void;
  const off = vi.fn();
  vi.spyOn(bridge.power, "onBattery").mockImplementation(async (handler) => {
    receive = handler;
    return off;
  });
  vi.spyOn(bridge.power, "battery").mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const started = subscribeBattery();
  await Promise.resolve();
  receive(battery);
  finish(null);
  const stop = await started;
  expect(useBattery.getState().battery).toEqual(battery);
  receive(null);
  expect(useBattery.getState().battery).toBeNull();
  stop();
  expect(off).toHaveBeenCalledOnce();
});

it("handles a failed initial read and recovers on a Windows power event", async () => {
  let receive!: (battery: BatteryReading | null) => void;
  vi.spyOn(bridge.power, "onBattery").mockImplementation(async (handler) => {
    receive = handler;
    return () => {};
  });
  vi.spyOn(bridge.power, "battery").mockRejectedValue(new Error("unavailable"));
  await subscribeBattery();
  expect(useBattery.getState().battery).toBeNull();
  receive(battery);
  expect(useBattery.getState().battery).toEqual(battery);
});
