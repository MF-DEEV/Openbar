// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { type Edge, type Geometry, edgeLocal, fromEdge, shoulder, notchMetrics } from "./geometry";
import type { Size } from "./motion";
import type { QuickButton, QuickSide } from "./settings";

export const QUICK_LAYOUT = { diameter: 44, offset: 34, row: 54, perSide: 3 } as const;
export const QUICK_SIDES: QuickSide[] = ["left", "right", "bottom"];
export interface PlacedQuickButton {
  button: QuickButton;
  cx: number;
  cy: number;
}

/**
 * The preview and the actual island use the same positions and capacity. On
 * another edge the top layout turns with the island: the left and right
 * columns sit at the panel's ends beside the screen edge, the bottom row on
 * the side facing the screen.
 */
export function placeButtons(
  buttons: QuickButton[],
  size: Size,
  geometry?: Geometry,
  edge: Edge = "top",
): PlacedQuickButton[] {
  const local = edgeLocal(size, edge);
  const shoulderWidth =
    geometry?.silhouette === "notch"
      ? notchMetrics(local.width, local.height, geometry.input).s
      : shoulder(local.height);
  const groups = QUICK_SIDES.map((side) => buttons.filter((b) => b.side === side).slice(0, QUICK_LAYOUT.perSide));
  const [left, right, bottom] = groups as [QuickButton[], QuickButton[], QuickButton[]];
  const span = (Math.max(left.length, right.length, 1) - 1) * QUICK_LAYOUT.row;
  const top = Math.max(QUICK_LAYOUT.diameter / 2 + 12, Math.min(52, local.height / 2 - span / 2));
  const placed = [
    ...left.map((button, i) => ({ button, cx: shoulderWidth - QUICK_LAYOUT.offset, cy: top + i * QUICK_LAYOUT.row })),
    ...right.map((button, i) => ({
      button,
      cx: local.width - shoulderWidth + QUICK_LAYOUT.offset,
      cy: top + i * QUICK_LAYOUT.row,
    })),
    ...bottom.map((button, i) => ({
      button,
      cx: local.width / 2 + (i - (bottom.length - 1) / 2) * QUICK_LAYOUT.row,
      cy: local.height + QUICK_LAYOUT.offset,
    })),
  ];
  if (edge === "top") return placed;
  return placed.map((p) => {
    const [cx, cy] = fromEdge(p.cx, p.cy, size, edge);
    return { ...p, cx, cy };
  });
}

export function freeQuickSide(buttons: QuickButton[], preferred?: QuickSide): QuickSide | undefined {
  return [preferred, ...QUICK_SIDES].find(
    (side): side is QuickSide => !!side && buttons.filter((b) => b.side === side).length < QUICK_LAYOUT.perSide,
  );
}
