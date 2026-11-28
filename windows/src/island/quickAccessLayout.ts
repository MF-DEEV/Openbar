// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { type Geometry, shoulder, notchMetrics } from "./geometry";
import type { Size } from "./motion";
import type { QuickButton, QuickSide } from "./settings";
import type { IslandEdge } from "./placement";

export const QUICK_LAYOUT = { diameter: 44, offset: 34, row: 54, perSide: 3 } as const;
export const QUICK_SIDES: QuickSide[] = ["left", "right", "bottom"];
export interface PlacedQuickButton {
  button: QuickButton;
  cx: number;
  cy: number;
}

/** The preview and the actual island use the same positions and capacity. */
export function placeButtons(buttons: QuickButton[], size: Size, geometry?: Geometry, edge: IslandEdge = "top", room?: { top: number; bottom: number }): PlacedQuickButton[] {
  const shoulderWidth =
    geometry?.silhouette === "notch" ? notchMetrics(size.width, size.height, geometry.input).s : shoulder(size.height);
  const groups = QUICK_SIDES.map((side) => buttons.filter((b) => b.side === side).slice(0, QUICK_LAYOUT.perSide));
  const [left, right, bottom] = groups as [QuickButton[], QuickButton[], QuickButton[]];
  const span = (Math.max(left.length, right.length, 1) - 1) * QUICK_LAYOUT.row;
  const top = edge === "left" || edge === "right"
    ? Math.max(QUICK_LAYOUT.diameter / 2 + 12, size.height / 2 - span / 2)
    : Math.max(QUICK_LAYOUT.diameter / 2 + 12, Math.min(52, size.height / 2 - span / 2));
  if (edge === "left" || edge === "right") {
    const direction = edge === "left" ? 1 : -1;
    if ((!room || (room.top >= 56 && room.bottom >= 56)) && size.height >= 216) {
      const pair = (buttons: QuickButton[], cy: number) => buttons.map((button, i) => ({
        button, cx: size.width / 2 + (i - (buttons.length - 1) / 2) * QUICK_LAYOUT.row, cy,
      }));
      const side = [bottom[0], left[2], right[2], bottom[1], bottom[2]].filter((button): button is QuickButton => !!button);
      return [
        ...pair([left[0], right[0]].filter((button): button is QuickButton => !!button), -QUICK_LAYOUT.offset),
        ...pair([left[1], right[1]].filter((button): button is QuickButton => !!button), size.height + QUICK_LAYOUT.offset),
        ...side.map((button, i) => ({ button,
          cx: (edge === "left" ? size.width : 0) + direction * QUICK_LAYOUT.offset,
          cy: size.height / 2 + (i - (side.length - 1) / 2) * QUICK_LAYOUT.row,
        })),
      ];
    }
    return groups.flatMap((group, column) => group.map((button, i) => ({
      button,
      cx: (edge === "left" ? size.width : 0) + direction * (QUICK_LAYOUT.offset + column * QUICK_LAYOUT.row),
      cy: top + i * QUICK_LAYOUT.row,
    })));
  }
  if (edge === "bottom") {
    const nearBottom = (group: QuickButton[]) => group.map((button, i) => ({
      button, cy: size.height - top - (group.length - 1 - i) * QUICK_LAYOUT.row,
    }));
    return [
      ...nearBottom(left).map(({ button, cy }) => ({ button, cx: shoulderWidth - QUICK_LAYOUT.offset, cy })),
      ...nearBottom(right).map(({ button, cy }) => ({ button, cx: size.width - shoulderWidth + QUICK_LAYOUT.offset, cy })),
      ...bottom.map((button, i) => ({
        button,
        cx: size.width / 2 + (i - (bottom.length - 1) / 2) * QUICK_LAYOUT.row,
        cy: -QUICK_LAYOUT.offset,
      })),
    ];
  }
  return [
    ...left.map((button, i) => ({ button, cx: shoulderWidth - QUICK_LAYOUT.offset, cy: top + i * QUICK_LAYOUT.row })),
    ...right.map((button, i) => ({
      button,
      cx: size.width - shoulderWidth + QUICK_LAYOUT.offset,
      cy: top + i * QUICK_LAYOUT.row,
    })),
    ...bottom.map((button, i) => ({
      button,
      cx: size.width / 2 + (i - (bottom.length - 1) / 2) * QUICK_LAYOUT.row,
      cy: size.height + QUICK_LAYOUT.offset,
    })),
  ];
}

export function freeQuickSide(buttons: QuickButton[], preferred?: QuickSide): QuickSide | undefined {
  return [preferred, ...QUICK_SIDES].find(
    (side): side is QuickSide => !!side && buttons.filter((b) => b.side === side).length < QUICK_LAYOUT.perSide,
  );
}
