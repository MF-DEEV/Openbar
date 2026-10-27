// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A color read from an entry that is only a color (ColorValue.swift): #RGB,
// #RGBA, #RRGGBB, #RRGGBBAA, rgb(), rgba(), hsl(), hsla() and SwiftUI's
// Color(red:green:blue:opacity:). A color inside longer text is not one,
// and a bare RRGGBB would also match numbers and hashes.

export interface Rgba {
  red: number;
  green: number;
  blue: number;
  alpha: number;
}

const MAX_LENGTH = 96;

function number(text: string): number | null {
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(text)) return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

/** "0%"…"100%" as a fraction. */
function percentage(text: string): number | null {
  if (!text.endsWith("%")) return null;
  const value = number(text.slice(0, -1));
  return value !== null && value >= 0 && value <= 100 ? value / 100 : null;
}

/** Opaque when absent; else 0…1 or a percentage. */
function alpha(text: string | undefined): number | null {
  if (text === undefined) return 1;
  const percent = percentage(text);
  if (percent !== null) return percent;
  const value = number(text);
  return value !== null && value >= 0 && value <= 1 ? value : null;
}

function args(value: string, names: string[]): string[] | null {
  if (!value.endsWith(")")) return null;
  const name = names.find((n) => value.startsWith(n + "("));
  if (!name) return null;
  return value
    .slice(name.length + 1, -1)
    .split(/[,/\s]+/)
    .filter(Boolean);
}

function hex(digits: string): Rgba | null {
  if (![3, 4, 6, 8].includes(digits.length) || !/^[0-9a-f]+$/.test(digits)) return null;
  const full = digits.length <= 4 ? [...digits].map((d) => d + d).join("") : digits;
  const byte = (i: number) => parseInt(full.slice(i * 2, i * 2 + 2), 16) / 255;
  return { red: byte(0), green: byte(1), blue: byte(2), alpha: full.length === 8 ? byte(3) : 1 };
}

function rgb(list: string[]): Rgba | null {
  if (list.length < 3 || list.length > 4) return null;
  const channels: number[] = [];
  for (const item of list.slice(0, 3)) {
    const percent = percentage(item);
    const value = number(item);
    if (percent !== null) channels.push(percent);
    else if (value !== null && value >= 0 && value <= 255) channels.push(value / 255);
    else return null;
  }
  const a = alpha(list[3]);
  return a === null ? null : { red: channels[0]!, green: channels[1]!, blue: channels[2]!, alpha: a };
}

function hsl(list: string[]): Rgba | null {
  if (list.length < 3 || list.length > 4) return null;
  const hue = number(list[0]!.endsWith("deg") ? list[0]!.slice(0, -3) : list[0]!);
  const s = percentage(list[1]!);
  const l = percentage(list[2]!);
  const a = alpha(list[3]);
  if (hue === null || s === null || l === null || a === null) return null;
  const h = (((hue % 360) + 360) % 360) / 360;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const channel = (offset: number) => {
    const k = (offset + h * 12) % 12;
    return l - (chroma / 2) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  return { red: channel(0), green: channel(8), blue: channel(4), alpha: a };
}

function swiftUI(inner: string): Rgba | null {
  const labels = ["red", "green", "blue", "opacity"];
  const pairs = inner.split(",").map((p) => p.split(":"));
  if (pairs.length < 3 || pairs.length > 4) return null;
  const values: number[] = [];
  for (let i = 0; i < pairs.length; i++) {
    const pair = pairs[i]!;
    if (pair.length !== 2 || pair[0]!.trim() !== labels[i]) return null;
    const value = number(pair[1]!.trim());
    if (value === null || value < 0 || value > 1) return null;
    values.push(value);
  }
  return { red: values[0]!, green: values[1]!, blue: values[2]!, alpha: values[3] ?? 1 };
}

export function parseColor(text: string): Rgba | null {
  // Checked cheaply first: every card asks on every render.
  if (text.length > MAX_LENGTH || new TextEncoder().encode(text).length > MAX_LENGTH) return null;
  const value = text.trim().toLowerCase();
  if (value.startsWith("#")) return hex(value.slice(1));
  const rgbArgs = args(value, ["rgba", "rgb"]);
  if (rgbArgs) return rgb(rgbArgs);
  const hslArgs = args(value, ["hsla", "hsl"]);
  if (hslArgs) return hsl(hslArgs);
  if (value.startsWith("color(") && value.endsWith(")")) return swiftUI(value.slice(6, -1));
  return null;
}

export function cssColor(c: Rgba): string {
  const byte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255);
  return `rgba(${byte(c.red)}, ${byte(c.green)}, ${byte(c.blue)}, ${c.alpha})`;
}
