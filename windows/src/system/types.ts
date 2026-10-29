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
  /** Full charge over design capacity, 0…100. */
  healthPercent: number | null;
  /** Null when the battery doesn't count its cycles. */
  cycleCount: number | null;
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
  /** CPU/GPU: percent. Memory: private bytes. Network: total bytes/sec. */
  value: number;
  netDown: number | null;
  netUp: number | null;
}

export type ProcessStatus =
  | "measuring"
  | "ready"
  | "unavailable"
  | "permissionDenied"
  | "incomplete"
  /** Network only: an administrator can allow it once (Allow button). */
  | "needsApproval";

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
  processStatus: ProcessStatus;
  /** The detail `processes` were read for. */
  processesFor: MetricKind | null;
  uptimeSeconds: number;
}

/** What speed_test.rs sends (SpeedTest.swift's published values). */
export type SpeedTestPhase = "idle" | "latency" | "download" | "upload" | "done" | "failed";

export interface SpeedTestState {
  phase: SpeedTestPhase;
  latencyMs: number | null;
  downloadMbps: number | null;
  uploadMbps: number | null;
  error: string | null;
}
