// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Readings for designing the System section in a plain browser (?system):
// a laptop on battery with a busy CPU, a nearly full disk and a minute of
// history behind each graph.

import type { SystemSnapshot } from "./types";

export function demoSnapshot(seq: number): SystemSnapshot {
  const wave = (base: number, swing: number, phase = 0) => base + swing * Math.sin(seq / 3 + phase);
  const memoryTotal = 16 * 1024 ** 3;
  return {
    seq,
    supported: { cpu: true, gpu: true, memory: true, network: true, disk: true, devices: true },
    cpu: Math.min(1, wave(0.82, 0.08)),
    gpu: wave(0.32, 0.12, 1),
    memoryUsed: Math.round(memoryTotal * wave(0.63, 0.02, 2)),
    memoryTotal,
    battery: {
      percent: 18,
      charging: false,
      pluggedIn: false,
      rateWatts: -wave(11.4, 2),
      secondsLeft: 4_200,
      healthPercent: 87.6,
      cycleCount: 412,
    },
    netDown: Math.max(0, wave(2_400_000, 1_800_000, 0.5)),
    netUp: Math.max(0, wave(180_000, 120_000, 2.5)),
    netTotalDown: 1_830_000_000 + seq * 2_000_000,
    netTotalUp: 214_000_000 + seq * 150_000,
    disk: { name: "C:", free: 41_200_000_000, total: 511_000_000_000 },
    diskRead: Math.max(0, wave(4_000_000, 3_500_000, 1.5)),
    diskWrite: Math.max(0, wave(1_100_000, 900_000, 0.2)),
    devices: [
      { name: "USB Receiver", bluetooth: false },
      { name: "Keychron K2", bluetooth: true },
      { name: "WH-1000XM5", bluetooth: true },
    ],
    processes: [
      { name: "Google Chrome", path: null, value: 21.4 },
      { name: "Visual Studio Code", path: null, value: 12.9 },
      { name: "Microsoft Teams", path: null, value: 6.2 },
      { name: "Desktop Window Manager", path: null, value: 3.1 },
      { name: "Vorssaint", path: null, value: 0.8 },
    ],
    processesFor: "cpu",
    uptimeSeconds: 3 * 86_400 + 4 * 3_600 + 12 * 60,
  };
}
