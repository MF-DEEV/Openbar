// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What system_monitor.rs sends (Snapshot), and the System section's metrics
// (MetricDetailKind in MetricDetailView.swift; fans have no Windows source).

export type MetricKind = "cpu" | "gpu" | "memory" | "battery" | "network" | "disk" | "power" | "connectedDevices";

/** The per-metric switches (AppFeature.monitor*): Power covers the battery and power cards. */
export type MonitorMetric = "cpu" | "gpu" | "memory" | "network" | "disk" | "power" | "connectedDevices";
export const MONITOR_METRICS: readonly MonitorMetric[] = [
  "cpu",
  "gpu",
  "memory",
  "network",
  "disk",
  "power",
  "connectedDevices",
];

export type NetworkSpeedUnit = "bytes" | "bits";

export interface Supported {
  cpu: boolean;
  gpu: boolean;
  memory: boolean;
  network: boolean;
  disk: boolean;
  devices: boolean;
}

export interface BatteryReading {
  percent: number | null;
  charging: boolean;
  pluggedIn: boolean;
  /** Watts into the battery: positive while charging, negative while it drains. */
  rateWatts: number | null;
  secondsLeft: number | null;
}

export interface DiskReading {
  /** "C:". */
  name: string;
  free: number;
  total: number;
}

export interface DeviceReading {
  name: string;
  bluetooth: boolean;
}

export interface ProcessRow {
  name: string;
  path: string | null;
  /** CPU: percent of the machine. Memory: private bytes. */
  value: number;
}

export interface SystemSnapshot {
  seq: number;
  supported: Supported;
  /** 0…1. */
  cpu: number | null;
  gpu: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  battery: BatteryReading | null;
  /** Bytes per second. */
  netDown: number | null;
  netUp: number | null;
  netTotalDown: number | null;
  netTotalUp: number | null;
  disk: DiskReading | null;
  diskRead: number | null;
  diskWrite: number | null;
  devices: DeviceReading[] | null;
  processes: ProcessRow[];
  /** The detail `processes` were read for. */
  processesFor: MetricKind | null;
  uptimeSeconds: number;
}
