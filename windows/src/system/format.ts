// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The monitor's number formats (MetricFormat.swift): byte sizes base 1024,
// disk sizes and bit rates base 1000, one decimal under ten.

/** A 0…1 fraction as a rounded percentage, e.g. "12%". */
export function percent(fraction: number): string {
  return `${Math.round(Math.max(0, Math.min(1, fraction)) * 100)}%`;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** A byte count as a value and unit, base 1024, promoted when it would print as 1024. */
export function scale(bytes: number): { value: number; unit: string } {
  let value = Math.max(0, bytes);
  let index = 0;
  while (value >= 1024 && index < BYTE_UNITS.length - 1) {
    value /= 1024;
    index++;
  }
  while (index < BYTE_UNITS.length - 1 && Math.round(value) >= 1024 && (index === 0 || value >= 10)) {
    value /= 1024;
    index++;
  }
  return { value, unit: BYTE_UNITS[index]! };
}

function number(value: number, unit: string): string {
  if (unit === "B") return value.toFixed(0);
  return value < 10 ? value.toFixed(1) : value.toFixed(0);
}

/** A quantity of bytes, e.g. "1.2 GB". */
export function bytes(n: number): string {
  const { value, unit } = scale(n);
  return `${number(value, unit)} ${unit}`;
}

/** A disk size as Explorer's capacity labels read it: base 1000. */
export function diskBytes(n: number): string {
  let value = Math.max(0, n);
  let index = 0;
  while (value >= 1000 && index < BYTE_UNITS.length - 1) {
    value /= 1000;
    index++;
  }
  const unit = BYTE_UNITS[index]!;
  if (unit === "B") return `${value.toFixed(0)} B`;
  return `${value < 10 ? value.toFixed(1) : value.toFixed(0)} ${unit}`;
}

/** A throughput, e.g. "1.2 MB/s". */
export function bytesPerSec(n: number): string {
  const { value, unit } = scale(n);
  return `${number(value, unit)} ${unit}/s`;
}

/** A compact throughput, e.g. "1.2M", "320K", "0B". */
export function bytesPerSecCompact(n: number): string {
  const units = ["B", "K", "M", "G", "T", "P"];
  let value = Number.isFinite(n) ? Math.max(0, n) : 0;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index++;
  }
  while (index < units.length - 1 && (index === 0 || value >= 10) && Math.round(value) >= 1024) {
    value /= 1024;
    index++;
  }
  return compact(value, units[index]!, index === 0);
}

/** A bit rate from bytes per second, e.g. "12 Mbps"; base 1000, as link speeds are quoted. */
export function bitsPerSec(n: number): string {
  const units = ["bps", "Kbps", "Mbps", "Gbps", "Tbps", "Pbps"];
  let value = Number.isFinite(n) ? Math.max(0, n) * 8 : 0;
  let index = 0;
  while (index < units.length - 1 && Math.round(value) >= 1000) {
    value /= 1000;
    index++;
  }
  if (index === 0) return `${value.toFixed(0)} ${units[0]}`;
  return `${value < 10 ? value.toFixed(1) : value.toFixed(0)} ${units[index]}`;
}

/** A compact bit rate, e.g. "9.6Mb", "320Kb", "0b". */
export function bitsPerSecCompact(n: number): string {
  const units = ["b", "Kb", "Mb", "Gb", "Tb", "Pb"];
  let value = Number.isFinite(n) ? Math.max(0, n) * 8 : 0;
  let index = 0;
  while (index < units.length - 1) {
    const shown = value < 10 && index > 0 ? Math.round(value * 10) / 10 : Math.round(value);
    if (shown < 1000) break;
    value /= 1000;
    index++;
  }
  return compact(value, units[index]!, index === 0);
}

function compact(value: number, unit: string, whole: boolean): string {
  if (whole) return `${Math.round(value)}${unit}`;
  if (value < 10) {
    const rounded = Math.round(value * 10) / 10;
    return rounded >= 10 ? `${Math.round(rounded)}${unit}` : `${rounded.toFixed(1)}${unit}`;
  }
  return `${Math.round(value)}${unit}`;
}

/** A network speed in the chosen unit (networkSpeedUnit). */
export const networkRate = (n: number, inBits: boolean) => (inBits ? bitsPerSec(n) : bytesPerSec(n));
export const networkRateCompact = (n: number, inBits: boolean) =>
  inBits ? bitsPerSecCompact(n) : bytesPerSecCompact(n);

/** Power, e.g. "8.5 W" / "23 W". */
export function watts(value: number): string {
  return Math.abs(value) < 10 ? `${value.toFixed(1)} W` : `${value.toFixed(0)} W`;
}

/** Compact power, e.g. "9W". */
export function wattsCompact(value: number): string {
  return `${Math.round(value)}W`;
}

/** "3d 4h", "2h 5min", "12min". */
export function uptime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const days = Math.floor(total / 86_400);
  const hours = Math.floor((total % 86_400) / 3_600);
  const minutes = Math.floor((total % 3_600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}min`;
  return `${minutes}min`;
}

/** "1h 25m" (BatteryTimeSupport.formatted); null for no estimate. */
export function batteryTime(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return null;
  const minutes = Math.max(1, Math.floor(seconds / 60));
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/** A graph's top: the peak rounded up to 1, 2 or 5 of a unit, so its label reads round. */
export function graphCeiling(peak: number, unitStep: number): number {
  for (let power = 0; power < 6; power++) {
    for (const step of [1, 2, 5, 10, 20, 50, 100, 200, 500]) {
      const ceiling = step * unitStep ** power;
      if (ceiling >= peak) return ceiling;
    }
  }
  return peak;
}

/** A network graph's top that lands on a round number in the chosen unit. */
export function networkGraphCeiling(peak: number, inBits: boolean): number {
  return inBits ? graphCeiling(peak * 8, 1000) / 8 : graphCeiling(peak, 1024);
}
