// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The grid's order and hidden set (PanelLayout.itemOrder, QuickLauncherService
// visibleItems / hiddenItems) and the island rail's layout (NotchLayout rails,
// NotchSupport.swift:533-583), kept pure for the tests.

import { TOGGLE_IDS, type ToggleId } from "./types";

/**
 * The stored order's known ids first, each once, then every id it doesn't
 * name in the default order: a tool added later joins a saved order at the end.
 */
export function orderedIds<T extends string>(stored: readonly string[], defaults: readonly T[]): T[] {
  const known = new Set<string>(defaults);
  const head = [...new Set(stored)].filter((id): id is T => known.has(id));
  return [...head, ...defaults.filter((id) => !head.includes(id))];
}

/** The available tools in order, split into the grid and the "Add" tray. */
export function splitTools<T extends string>(
  stored: readonly string[],
  defaults: readonly T[],
  hidden: readonly string[],
  available: (id: T) => boolean,
): { visible: T[]; hidden: T[] } {
  const ordered = orderedIds(stored, defaults).filter(available);
  return { visible: ordered.filter((id) => !hidden.includes(id)), hidden: ordered.filter((id) => hidden.includes(id)) };
}

/**
 * The full order after the grid's visible tools were rearranged: their slots
 * take the new sequence, and hidden or unavailable tools keep theirs.
 */
export function mergedOrder<T extends string>(full: readonly T[], visible: readonly T[]): T[] {
  const moved = new Set(visible);
  let next = 0;
  return full.map((id) => (moved.has(id) ? visible[next++]! : id));
}

/** The quick toggles in the person's order (panelToggleOrder). */
export const toggleOrder = (settings: { togglesOrder: readonly string[] }): ToggleId[] =>
  orderedIds(settings.togglesOrder, TOGGLE_IDS);

/** Hiding or showing one tool; the stored list stays sorted, as the Mac's CSV is. */
export function withHidden(hidden: readonly string[], id: string, hide: boolean): string[] {
  const set = new Set(hidden);
  if (hide) set.add(id);
  else set.delete(id);
  return [...set].sort();
}

// MARK: Rail

export const TOOL_WIDTH = 76;
export const TOOL_HEIGHT = 72;
export const TOOL_SPACING = 6;
const TOOL_LEAST_SPACING = 0;
/** NotchLayout.emptyHeight */
export const EMPTY_HEIGHT = 140;
/** NotchLayout.toolColumns: edit mode's grid. */
export const EDIT_COLUMNS = 5;

export const railCapacity = (width: number, item: number, spacing: number) =>
  Number.isFinite(width) && item > 0 ? Math.max(1, Math.floor((width + spacing) / (item + spacing))) : 1;

export function railRows(count: number, perRow: number, rowHeight: number, spacing: number, height: number): number {
  const needed = Math.max(1, Math.ceil(Math.max(0, count) / Math.max(1, perRow)));
  const fitting = Math.max(1, Math.floor((height + spacing) / (rowHeight + spacing)));
  return Math.min(needed, fitting);
}

export const railHeight = (rows: number) => Math.max(1, rows) * TOOL_HEIGHT + Math.max(0, rows - 1) * TOOL_SPACING;

export interface RailLayout {
  rows: number;
  columns: number;
  /** The room between columns when every column fits; null when the rail scrolls. */
  gap: number | null;
  /** Where the gaps closed, each title gives up half the lost gap on either side. */
  titleInset: number;
}

/** How `count` tools lay out in `width` × `height` (NotchRail, toolTitleInset). */
export function railLayout(count: number, width: number, height: number): RailLayout {
  const rows = railRows(count, railCapacity(width, TOOL_WIDTH, TOOL_SPACING), TOOL_HEIGHT, TOOL_SPACING, height);
  const columns = Math.ceil(Math.max(0, count) / rows);
  const fits = (spacing: number) => columns * TOOL_WIDTH + Math.max(0, columns - 1) * spacing <= width;
  let gap: number | null = null;
  if (fits(TOOL_SPACING)) gap = TOOL_SPACING;
  else if (columns > 1 && fits(TOOL_LEAST_SPACING)) gap = (width - columns * TOOL_WIDTH) / (columns - 1);
  const titleInset = Math.max(0, TOOL_SPACING - (gap ?? TOOL_SPACING)) / 2;
  return { rows, columns, gap, titleInset };
}

export type Direction = "up" | "down" | "left" | "right";

/**
 * The arrows walk the tiles the way the rail draws them, clamped, never
 * wrapping (QuickToolsSupport.gridIndex): reading order while the columns
 * fit, down the columns once it scrolls.
 */
export function moveIndex(index: number, count: number, layout: RailLayout, direction: Direction): number {
  if (count <= 0) return 0;
  const byRows = layout.gap !== null;
  const across = byRows ? layout.columns : layout.rows;
  const turned: Direction = byRows
    ? direction
    : ({ up: "left", down: "right", left: "up", right: "down" } as const)[direction];
  const current = Math.min(Math.max(index, 0), count - 1);
  const step = { left: -1, right: 1, up: -across, down: across }[turned];
  const candidate = current + step;
  if (candidate < 0 || candidate >= count) return current;
  if (turned === "left" && current % across === 0) return current;
  if (turned === "right" && current % across === across - 1) return current;
  return candidate;
}
