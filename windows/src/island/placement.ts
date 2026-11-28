// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Size } from "./motion";

export type IslandPosition = { x: number; y: number };
export type Placement = { left: number; top: number };
export type UsableArea = { left?: number; right?: number; top: number; bottom: number };
export type IslandEdge = "top" | "bottom" | "left" | "right";

/** Move only the selected work-area edge inward by the capsule's saved gap. */
export function insetUsableEdge(usable: UsableArea, edge: IslandEdge, distance: number, screen: Size): UsableArea {
  const left = usable.left ?? 0;
  const right = usable.right ?? screen.width;
  const horizontal = Math.min(Math.max(0, distance), Math.max(0, right - left));
  const vertical = Math.min(Math.max(0, distance), Math.max(0, usable.bottom - usable.top));
  switch (edge) {
    case "top":
      return { ...usable, top: usable.top + vertical };
    case "bottom":
      return { ...usable, bottom: usable.bottom - vertical };
    case "left":
      return { ...usable, left: left + horizontal };
    case "right":
      return { ...usable, right: right - horizontal };
  }
}

const clamp = (value: number, low: number, high: number) => Math.min(Math.max(value, low), Math.max(low, high));

/** Store a point on the usable perimeter. A small corner inset keeps its edge unambiguous. */
export function constrainIslandPosition(
  position: IslandPosition,
  screen: Size,
  usable: UsableArea = { top: 0, bottom: screen.height },
  previous?: IslandEdge,
): IslandPosition {
  const x = clamp(Number.isFinite(position.x) ? position.x : 0.5, 0, 1);
  const y = clamp(Number.isFinite(position.y) ? position.y : 0, 0, 1);
  const width = Math.max(1, (usable.right ?? screen.width) - (usable.left ?? 0));
  const height = Math.max(1, usable.bottom - usable.top);
  const distances: Record<IslandEdge, number> = {
    top: y * height,
    bottom: (1 - y) * height,
    left: x * width,
    right: (1 - x) * width,
  };
  const nearest = islandEdge({ x, y }, screen, usable);
  const edge = previous && distances[previous] <= distances[nearest] + 10 ? previous : nearest;
  // The actual frame size is clamped by placeIsland; this inset only prevents
  // a point at an exact corner from changing edge on the next render.
  const cornerX = Math.min(0.25, 1 / width);
  const cornerY = Math.min(0.25, 1 / height);
  if (edge === "top") return { x: clamp(x, cornerX, 1 - cornerX), y: 0 };
  if (edge === "bottom") return { x: clamp(x, cornerX, 1 - cornerX), y: 1 };
  if (edge === "left") return { x: 0, y: clamp(y, cornerY, 1 - cornerY) };
  return { x: 1, y: clamp(y, cornerY, 1 - cornerY) };
}

/** The nearest edge selects the panel's orientation and direction of growth. */
export function islandEdge(position: IslandPosition, screen: Size, usable: UsableArea): IslandEdge {
  const width = (usable.right ?? screen.width) - (usable.left ?? 0);
  const height = usable.bottom - usable.top;
  const distances: [IslandEdge, number][] = [
    ["top", position.y * height],
    ["bottom", (1 - position.y) * height],
    ["left", position.x * width],
    ["right", (1 - position.x) * width],
  ];
  return distances.reduce((best, next) => (next[1] < best[1] ? next : best))[0];
}

/** The painted shape stays against the edge from which it opens. */
export function surfaceOffset(
  target: Size,
  drawn: Size,
  edge: IslandEdge,
  position: IslandPosition = { x: 0.5, y: 0.5 },
): { x: number; y: number } {
  return {
    x: edge === "left" ? 0 : edge === "right" ? target.width - drawn.width : (target.width - drawn.width) * position.x,
    y:
      edge === "bottom"
        ? target.height - drawn.height
        : edge === "left" || edge === "right"
          ? (target.height - drawn.height) * position.y
          : 0,
  };
}

/** Stored coordinates run along the closed island's available travel on each edge. */
export function placeIsland(
  position: IslandPosition,
  screen: Size,
  size: Size,
  usable: UsableArea = { top: 0, bottom: screen.height },
  bubbles = false,
  closedHeight = size.height,
  closedWidth = size.width,
): Placement {
  position = constrainIslandPosition(position, screen, usable);
  const edge = islandEdge(position, screen, usable);
  // Room beside the panel for the quick buttons that sit off its ends.
  const gutter = bubbles && (edge === "top" || edge === "bottom") ? 56 : 0;
  const sideGutter =
    bubbles && (edge === "left" || edge === "right") && usable.bottom - usable.top - size.height >= 112 ? 56 : 0;
  const workLeft = usable.left ?? 0;
  const workRight = usable.right ?? screen.width;
  const closedLeft = workLeft + position.x * Math.max(0, workRight - workLeft - closedWidth);
  const closedTop = usable.top + position.y * Math.max(0, usable.bottom - usable.top - closedHeight);
  const wantedLeft =
    edge === "left"
      ? closedLeft
      : edge === "right"
        ? closedLeft + closedWidth - size.width
        : closedLeft + (closedWidth - size.width) / 2;
  const left = clamp(wantedLeft, workLeft + gutter, workRight - size.width - gutter);
  const available = Math.max(0, usable.bottom - usable.top - size.height - 2 * sideGutter);
  const wanted =
    edge === "bottom"
      ? closedTop + closedHeight - size.height
      : edge === "left" || edge === "right"
        ? closedTop + (closedHeight - size.height) / 2
        : closedTop;
  const top = clamp(wanted, usable.top + sideGutter, usable.top + sideGutter + available);
  return { left, top };
}

/** Browser work area is in screen coordinates; translate it to the island canvas. */
export function usableDisplayArea(screen: Size): Required<UsableArea> {
  const display = window.screen as Screen & { top?: number; left?: number; availTop?: number; availLeft?: number };
  const originY = display.top ?? 0;
  const originX = display.left ?? 0;
  const top = Math.max(0, (display.availTop ?? originY) - originY);
  const left = Math.max(0, (display.availLeft ?? originX) - originX);
  const bottom = Math.min(screen.height, top + (display.availHeight || screen.height));
  const right = Math.min(screen.width, left + (display.availWidth || screen.width));
  return { left, right: Math.max(left, right), top, bottom: Math.max(top, bottom) };
}
