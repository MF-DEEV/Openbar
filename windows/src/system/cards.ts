// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The cards the page shows, in the Mac's order (NotchSystemView.cards,
// :27-87). A card the machine has no reading for, or the person switched
// off, is left out; a value not read yet shows as "…".

import { networkRate, percent, wattsCompact, diskBytes } from "./format";
import { S } from "./strings";
import type { BatteryReading, MetricKind, MonitorMetric, SystemSnapshot } from "./types";

export interface Card {
  kind: MetricKind;
  title: string;
  /** Null until read: the card shows "…". */
  value: string | null;
  /** A second line in place of the meter (network upload, power state). */
  detail?: string | null;
  /** 0…1 for the meter. */
  level?: number | null;
  /** The reading itself is the news: the meter turns orange. */
  attention: boolean;
  /** The battery card's glyph shows the plug. */
  charging?: boolean;
}

/** CPU, GPU and memory at 85 % or more. */
export function isBusy(level: number | null | undefined): boolean {
  return level !== null && level !== undefined && Number.isFinite(level) && level >= 0.85;
}

/** Battery at 20 % or less, unplugged. */
export function batteryLow(battery: BatteryReading): boolean {
  return !battery.pluggedIn && (battery.percent ?? 100) <= 20;
}

/** Less than 10 % of the disk free. */
export function diskLow(free: number, total: number): boolean {
  return total > 0 && free / total < 0.1;
}

const fraction = (value: number | null) => (value !== null && Number.isFinite(value) ? value : null);
const percentText = (value: number | null) => (fraction(value) === null ? null : percent(value!));

/**
 * The power card, shown while the battery reports a rate (so it doesn't blink
 * as the rate touches zero): the draw while the battery feeds the PC, the
 * charge going in while plugged in. Windows has no system or adapter wattage.
 */
export function powerReading(battery: BatteryReading | null): { watts: number; pluggedIn: boolean } | null {
  const rate = battery?.rateWatts;
  if (rate === null || rate === undefined || !Number.isFinite(rate)) return null;
  if (battery!.pluggedIn) return { watts: Math.max(0, rate), pluggedIn: true };
  return { watts: Math.max(0, -rate), pluggedIn: false };
}

export function systemCards(
  snapshot: SystemSnapshot | null,
  options: { hidden: readonly MonitorMetric[]; inBits: boolean },
): Card[] {
  if (!snapshot) return [];
  const on = (metric: MonitorMetric) => !options.hidden.includes(metric);
  const { supported } = snapshot;
  const cards: Card[] = [];
  if (supported.cpu && on("cpu")) {
    cards.push({
      kind: "cpu",
      title: S.cpu,
      value: percentText(snapshot.cpu),
      level: fraction(snapshot.cpu),
      attention: isBusy(snapshot.cpu),
    });
  }
  if (supported.gpu && on("gpu")) {
    cards.push({
      kind: "gpu",
      title: S.gpu,
      value: percentText(snapshot.gpu),
      level: fraction(snapshot.gpu),
      attention: isBusy(snapshot.gpu),
    });
  }
  if (supported.memory && on("memory")) {
    const { memoryUsed: used, memoryTotal: total } = snapshot;
    const ratio = used !== null && total !== null && total > 0 ? used / total : null;
    cards.push({ kind: "memory", title: S.memory, value: percentText(ratio), level: ratio, attention: isBusy(ratio) });
  }
  const battery = snapshot.battery;
  if (battery && on("power")) {
    cards.push({
      kind: "battery",
      title: S.battery,
      value: battery.percent === null ? null : `${battery.percent}%`,
      level: battery.percent === null ? null : battery.percent / 100,
      attention: batteryLow(battery),
      charging: battery.pluggedIn,
    });
  }
  if (supported.network && on("network")) {
    cards.push({
      kind: "network",
      title: S.network,
      value: snapshot.netDown === null ? null : `↓ ${networkRate(snapshot.netDown, options.inBits)}`,
      detail: snapshot.netUp === null ? null : `↑ ${networkRate(snapshot.netUp, options.inBits)}`,
      attention: false,
    });
  }
  if (supported.disk && on("disk")) {
    const disk = snapshot.disk;
    cards.push({
      kind: "disk",
      title: S.available,
      value: disk ? diskBytes(disk.free) : null,
      level: disk && disk.total > 0 ? disk.free / disk.total : null,
      attention: disk ? diskLow(disk.free, disk.total) : false,
    });
  }
  const power = powerReading(battery);
  if (power && on("power")) {
    cards.push({
      kind: "power",
      title: S.power,
      value: wattsCompact(power.watts),
      detail: power.pluggedIn ? (battery!.charging ? S.charging : S.pluggedIn) : null,
      attention: false,
    });
  }
  if (supported.devices && on("connectedDevices")) {
    cards.push({
      kind: "connectedDevices",
      title: S.connectedDevices,
      value: snapshot.devices ? String(snapshot.devices.length) : null,
      attention: false,
    });
  }
  return cards;
}
