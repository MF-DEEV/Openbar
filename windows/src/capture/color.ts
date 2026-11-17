// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The loupe's readout shows exactly what a click copies: the same formats as
// capture/color.rs (ColorValue.string on the Mac), which does the copying.

export type ColorFormat = "hex" | "rgb" | "hsl";

export function hsl(r: number, g: number, b: number): [number, number, number] {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255];
  const max = Math.max(rr, gg, bb);
  const min = Math.min(rr, gg, bb);
  const delta = max - min;
  const light = (max + min) / 2;
  if (delta <= 0.000001) return [0, 0, light];
  const sat = delta / (1 - Math.abs(2 * light - 1));
  let hue = max === rr ? ((gg - bb) / delta) % 6 : max === gg ? (bb - rr) / delta + 2 : (rr - gg) / delta + 4;
  hue *= 60;
  if (hue < 0) hue += 360;
  return [hue, Math.min(1, Math.max(0, sat)), light];
}

export function formatColor(r: number, g: number, b: number, format: ColorFormat, bareHex: boolean): string {
  const hex = (n: number) => n.toString(16).toUpperCase().padStart(2, "0");
  switch (format) {
    case "hex":
      return `${bareHex ? "" : "#"}${hex(r)}${hex(g)}${hex(b)}`;
    case "rgb":
      return `rgb(${r}, ${g}, ${b})`;
    case "hsl": {
      const [h, s, l] = hsl(r, g, b);
      return `hsl(${Math.round(h)}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%)`;
    }
  }
}
