// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What a metric's detail reads (MetricDetailView.swift): the summary's value
// and line, its history graph and its rows. Temperatures, memory pressure,
// battery health and cycles, adapter watts and the speed test have no
// Windows source and are left out.

import { powerReading } from "./cards";
import {
  bytes,
  bytesPerSec,
  batteryTime,
  diskBytes,
  graphCeiling,
  networkGraphCeiling,
  networkRate,
  networkRateCompact,
  percent,
  uptime,
  watts,
  wattsCompact,
} from "./format";
import type { Line } from "./Sparkline";
import type { History } from "./store";
import { S } from "./strings";
import type { BatteryReading, MetricKind, ProcessRow, SystemSnapshot } from "./types";

/** The panel's metric colors in dark appearance (PanelMetricColor). */
export const COLORS = {
  accent: "#0a84ff",
  cyan: "#64d2ff",
  mint: "#66d4cf",
  yellow: "#ffd60a",
  green: "#30d158",
  orange: "#ff9f0a",
  pink: "#ff375f",
} as const;

export const TITLES: Record<MetricKind, string> = {
  cpu: S.cpu,
  gpu: S.gpu,
  memory: S.memory,
  battery: S.battery,
  network: S.network,
  disk: S.disks,
  power: S.power,
  connectedDevices: S.connectedDevices,
};

export interface Summary {
  primary: string;
  secondary: string;
  color: string;
  graph: { lines: Line[]; max: number; label: string } | null;
}

export interface Row {
  title: string;
  value: string;
  /** A device row's glyph. */
  bluetooth?: boolean;
}

const usedFraction = (s: SystemSnapshot) =>
  s.memoryUsed !== null && s.memoryTotal ? s.memoryUsed / s.memoryTotal : null;
const diskUsed = (s: SystemSnapshot) => (s.disk && s.disk.total > 0 ? 1 - s.disk.free / s.disk.total : null);
const peak = (...series: number[][]) => Math.max(1, ...series.flat());

function powerState(battery: BatteryReading | null): string {
  if (!battery) return S.powerUnavailable;
  if (battery.charging) return S.charging;
  return battery.pluggedIn ? S.pluggedIn : S.onBattery;
}

/** "8.5 W · Charging" (MetricDetailView.batteryFlowText). */
function batteryFlow(battery: BatteryReading | null): string {
  const rate = battery?.rateWatts;
  if (rate === null || rate === undefined) return "-";
  return `${watts(Math.abs(rate))} · ${rate >= 0 ? S.charging : S.onBattery}`;
}

const percentGraph = (values: number[], color: string) => ({ lines: [{ values, color }], max: 1, label: percent(1) });

export function summary(kind: MetricKind, s: SystemSnapshot, history: History, inBits: boolean): Summary {
  switch (kind) {
    case "cpu":
    case "gpu": {
      const value = kind === "cpu" ? s.cpu : s.gpu;
      const color = kind === "cpu" ? COLORS.accent : COLORS.cyan;
      return {
        primary: value === null ? "-" : percent(value),
        secondary: S.usage,
        color,
        graph: percentGraph(history[kind], color),
      };
    }
    case "memory": {
      const ratio = usedFraction(s);
      return {
        primary: ratio === null ? "-" : percent(ratio),
        secondary: ratio === null ? S.memory : `${bytes(s.memoryUsed!)} / ${bytes(s.memoryTotal!)}`,
        color: COLORS.mint,
        graph: percentGraph(history.memory, COLORS.mint),
      };
    }
    case "network": {
      const top = networkGraphCeiling(peak(history.netDown, history.netUp), inBits);
      return {
        primary: s.netDown === null ? "-" : networkRateCompact(s.netDown, inBits),
        secondary: `${S.upload} ${s.netUp === null ? "-" : networkRateCompact(s.netUp, inBits)}`,
        color: COLORS.accent,
        graph: {
          lines: [
            { values: history.netDown, color: COLORS.accent },
            { values: history.netUp, color: COLORS.green, fillOpacity: 0.08 },
          ],
          max: top,
          label: networkRate(top, inBits),
        },
      };
    }
    case "disk": {
      const used = diskUsed(s);
      const top = graphCeiling(peak(history.diskRead, history.diskWrite), 1024);
      return {
        primary: used === null ? "-" : percent(used),
        secondary: s.disk ? `${diskBytes(s.disk.free)} ${S.availableRow}` : S.noDisks,
        color: COLORS.yellow,
        graph: {
          lines: [
            { values: history.diskRead, color: COLORS.yellow },
            { values: history.diskWrite, color: COLORS.pink, fillOpacity: 0.08 },
          ],
          max: top,
          label: bytesPerSec(top),
        },
      };
    }
    case "battery":
      return {
        primary: s.battery?.percent === null || s.battery?.percent === undefined ? "-" : `${s.battery.percent}%`,
        secondary: powerState(s.battery),
        color: COLORS.green,
        graph: percentGraph(history.battery, COLORS.green),
      };
    case "power": {
      const power = powerReading(s.battery);
      const top = graphCeiling(Math.max(0, ...history.power), 1000);
      return {
        primary: power ? wattsCompact(power.watts) : "-",
        secondary: powerState(s.battery),
        color: COLORS.orange,
        graph: { lines: [{ values: history.power, color: COLORS.orange }], max: top, label: watts(top) },
      };
    }
    case "connectedDevices": {
      const count = s.devices?.length ?? 0;
      return { primary: String(count), secondary: S.devicesConnected(count), color: COLORS.cyan, graph: null };
    }
  }
}

export function detailRows(kind: MetricKind, s: SystemSnapshot, inBits: boolean): Row[] {
  const measured = (value: string | null) => value ?? S.measuring;
  switch (kind) {
    case "cpu":
      return [
        { title: S.usage, value: measured(s.cpu === null ? null : percent(s.cpu)) },
        { title: S.upFor, value: uptime(s.uptimeSeconds) },
      ];
    case "gpu":
      return [{ title: S.usage, value: measured(s.gpu === null ? null : percent(s.gpu)) }];
    case "memory":
      return [
        {
          title: S.memory,
          value: `${s.memoryUsed === null ? S.measuring : bytes(s.memoryUsed)} / ${s.memoryTotal === null ? "-" : bytes(s.memoryTotal)}`,
        },
      ];
    case "network":
      return [
        { title: S.download, value: measured(s.netDown === null ? null : networkRate(s.netDown, inBits)) },
        { title: S.upload, value: measured(s.netUp === null ? null : networkRate(s.netUp, inBits)) },
        {
          title: S.thisSession,
          value:
            s.netTotalDown === null || s.netTotalUp === null
              ? "-"
              : `↓${bytes(s.netTotalDown)}  ↑${bytes(s.netTotalUp)}`,
        },
      ];
    case "disk": {
      if (!s.disk) return [{ title: S.disks, value: S.noDisks }];
      return [
        { title: s.disk.name, value: `${percent(diskUsed(s) ?? 0)} ${S.used}` },
        { title: S.available, value: diskBytes(s.disk.free) },
        { title: S.read, value: measured(s.diskRead === null ? null : bytesPerSec(s.diskRead)) },
        { title: S.write, value: measured(s.diskWrite === null ? null : bytesPerSec(s.diskWrite)) },
      ];
    }
    case "battery":
      return [
        { title: S.charge, value: measured(s.battery?.percent == null ? null : `${s.battery.percent}%`) },
        { title: S.battery, value: batteryFlow(s.battery) },
      ];
    case "power": {
      const battery = s.battery;
      const power = powerReading(battery);
      const rows: Row[] = [
        { title: S.system, value: power && !power.pluggedIn ? watts(power.watts) : "-" },
        { title: S.adapter, value: battery?.pluggedIn ? S.pluggedIn : "-" },
        { title: S.battery, value: batteryFlow(battery) },
      ];
      if (battery && !battery.pluggedIn && !battery.charging) {
        rows.push({ title: S.batteryTime, value: batteryTime(battery.secondsLeft) ?? S.calculating });
      }
      return rows;
    }
    case "connectedDevices": {
      const devices = s.devices ?? [];
      if (devices.length === 0) return [{ title: S.noDevices, value: "" }];
      return devices.map((d) => ({
        title: d.name || (d.bluetooth ? S.unnamedBluetooth : S.unnamedUsb),
        value: "",
        bluetooth: d.bluetooth,
      }));
    }
  }
}

/** CPU rows read "12.3%", memory rows a size. */
export function processValue(kind: MetricKind, value: number): string {
  return kind === "memory" ? bytes(value) : `${value.toFixed(1)}%`;
}

export function processRowValue(kind: MetricKind, row: ProcessRow, inBits: boolean): string {
  if (kind !== "network") return processValue(kind, row.value);
  const rate = (value: number | null) => (value === null ? "-" : networkRateCompact(value, inBits));
  return `↓${rate(row.netDown)} ↑${rate(row.netUp)}`;
}

/** A stale response belongs to the previous detail, even if it had rows. */
export function processEmptyText(kind: MetricKind, snapshot: SystemSnapshot): string | null {
  if (snapshot.processesFor !== kind) return S.measuring;
  switch (snapshot.processStatus) {
    case "unavailable":
      return kind === "power" ? S.energyUnavailable : S.processesUnavailable;
    case "permissionDenied":
      return S.networkPermissionDenied;
    case "incomplete":
      return S.networkIncomplete;
    case "measuring":
      return S.measuring;
    case "ready":
      return snapshot.processes.length > 0 ? null : kind === "network" ? S.networkAppsIdle : S.processesIdle;
  }
}
