// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The System section's state: the latest reading, the recent history its
// detail graphs draw, and the metric detail open on the page
// (NotchService.selectedMetric).

import { create } from "zustand";
import { useIsland } from "../island/store";
import { powerReading } from "./cards";
import type { MetricKind, SystemSnapshot } from "./types";

/** Samples kept per series: two minutes at the default 2 s. */
export const HISTORY_LENGTH = 60;

export type Series = "cpu" | "gpu" | "memory" | "netDown" | "netUp" | "diskRead" | "diskWrite" | "battery" | "power";
export type History = Record<Series, number[]>;

export const emptyHistory = (): History => ({
  cpu: [],
  gpu: [],
  memory: [],
  netDown: [],
  netUp: [],
  diskRead: [],
  diskWrite: [],
  battery: [],
  power: [],
});

/** Each series gains the reading's value, or keeps its last one when there is none, oldest dropped past the cap. */
export function pushHistory(history: History, s: SystemSnapshot): History {
  const values: Record<Series, number | null> = {
    cpu: s.cpu,
    gpu: s.gpu,
    memory: s.memoryUsed !== null && s.memoryTotal ? s.memoryUsed / s.memoryTotal : null,
    netDown: s.netDown,
    netUp: s.netUp,
    diskRead: s.diskRead,
    diskWrite: s.diskWrite,
    battery: s.battery?.percent === null || s.battery?.percent === undefined ? null : s.battery.percent / 100,
    power: powerReading(s.battery)?.watts ?? null,
  };
  const next = {} as History;
  for (const key of Object.keys(values) as Series[]) {
    const value = values[key];
    next[key] =
      value === null || !Number.isFinite(value) ? history[key] : [...history[key], value].slice(-HISTORY_LENGTH);
  }
  return next;
}

interface SystemState {
  snapshot: SystemSnapshot | null;
  history: History;
  detail: MetricKind | null;
  receive(snapshot: SystemSnapshot): void;
  openDetail(kind: MetricKind): void;
  closeDetail(): void;
}

export const useSystem = create<SystemState>()((set, get) => ({
  snapshot: null,
  history: emptyHistory(),
  detail: null,
  receive(snapshot) {
    const previous = get().snapshot;
    // A reading seen before (the stored one fetched on opening) adds no history.
    const fresh = snapshot.seq > 0 && snapshot.seq !== previous?.seq;
    set({ snapshot, history: fresh ? pushHistory(get().history, snapshot) : get().history });
  },
  openDetail: (detail) => set({ detail }),
  closeDetail: () => set({ detail: null }),
}));

/** System's network reading (the Controls speed test tile). The page first:
 * a detail opened while System isn't showing is closed at once. */
export function showNetworkReading() {
  useIsland.getState().select("system");
  useSystem.getState().openDetail("network");
}
