// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Which home-page items show and how the page lays them out
// (NotchSupport.controls, NotchLayout.controls and the rail functions,
// NotchSupport.swift:494-675, 2060-2069; spec 05 §1.1-§1.2).

import type { ModuleId } from "../island/modules";
import { CONTROL_ITEMS, type ControlItem } from "./types";

export const ROW_SPACING = 10;
export const CARD_HEIGHT = 80;
export const MINIMUM_CARD_HEIGHT = 68;
export const BAND_LEVEL_MINIMUM_WIDTH = 76;
export const BAND_LEVEL_SPACING = 12;
export const LEVEL_ROW_HEIGHT = 28;
export const SHORTCUT_WIDTH = 76;
export const SHORTCUT_HEIGHT = 74;
export const SHORTCUT_SPACING = 8;
export const SHORTCUT_LEAST_SPACING = 0;
export const SHORTCUT_LEAST_ROW_SPACING = 6;
/** The island's sections button and the row's spacing after it (NotchBand.sectionsButtonRoom). */
export const SECTIONS_BUTTON_ROOM = 28 + 6;
export const EMPTY_HEIGHT = 140;

/**
 * Not on Windows: no generic API for a keyboard light, and recording, the
 * app panel and the command bar aren't ported (spec 05 §1.3).
 */
const UNSUPPORTED: ControlItem[] = ["keyboardLight", "recording", "panel", "commandBar"];

/** The section a tile needs to be shown (NotchControlItem.isAvailable). */
const NEEDS: Partial<Record<ControlItem, ModuleId>> = {
  music: "music",
  mixer: "mixer",
  timer: "timer",
  calendar: "calendar",
  speedTest: "system",
  scratchpad: "scratchpad",
};

export const isLevel = (item: ControlItem) => item === "volume" || item === "brightness" || item === "keyboardLight";
export const isShortcut = (item: ControlItem) => !isLevel(item) && item !== "music";

export function isAvailable(item: ControlItem, modules: readonly ModuleId[]): boolean {
  if (UNSUPPORTED.includes(item)) return false;
  const needs = NEEDS[item];
  return needs === undefined || modules.includes(needs);
}

/** Stored order first, then every other item in declaration order, minus the hidden and unavailable. */
export function controlsOf(
  order: readonly ControlItem[],
  hidden: readonly ControlItem[],
  modules: readonly ModuleId[],
): ControlItem[] {
  const all = [...new Set([...order, ...CONTROL_ITEMS])].filter((i) => CONTROL_ITEMS.includes(i));
  return all.filter((item) => !hidden.includes(item) && isAvailable(item, modules));
}

/** Every tile the settings editor offers, in order, shown or not (NotchSettings.orderedShortcuts). */
export function editableShortcuts(order: readonly ControlItem[]): ControlItem[] {
  return [...new Set([...order, ...CONTROL_ITEMS])].filter(
    (i) => CONTROL_ITEMS.includes(i) && isShortcut(i) && !UNSUPPORTED.includes(i),
  );
}

// MARK: Rails

export function railCapacity(width: number, itemWidth: number, spacing: number): number {
  if (!Number.isFinite(width) || itemWidth <= 0) return 1;
  return Math.max(1, Math.floor((width + spacing) / (itemWidth + spacing)));
}

export function railRows(count: number, perRow: number, rowHeight: number, spacing: number, height: number): number {
  const capacity = Math.max(1, perRow);
  const needed = Math.max(1, Math.ceil(Math.max(0, count) / capacity));
  if (rowHeight <= 0) return 1;
  if (!Number.isFinite(height)) return height > 0 ? needed : 1;
  const fitting = Math.max(1, Math.floor((height + spacing) / (rowHeight + spacing)));
  return Math.min(needed, fitting);
}

export function railHeight(rows: number, rowHeight: number, spacing: number): number {
  return Math.max(1, rows) * rowHeight + Math.max(0, rows - 1) * spacing;
}

/** Items per row: a rail that fits lays them out in reading order. */
export function railColumns(count: number, rows: number): number {
  return Math.ceil(Math.max(0, count) / Math.max(1, rows));
}

export function railFits(columns: number, itemWidth: number, spacing: number, width: number): boolean {
  return columns * itemWidth + Math.max(0, columns - 1) * spacing <= width;
}

/** Its own spacing, or what's left down to `least` before it scrolls; null when it scrolls. */
export function railGap(
  columns: number,
  itemWidth: number,
  spacing: number,
  least: number,
  width: number,
): number | null {
  if (railFits(columns, itemWidth, spacing, width)) return spacing;
  if (columns <= 1 || !railFits(columns, itemWidth, least, width)) return null;
  return (width - columns * itemWidth) / (columns - 1);
}

// MARK: The page

export interface ControlsLayout {
  levelRow: number;
  cardRow: number;
  shortcutRows: number;
  shortcutRowSpacing: number;
}

export function layoutHeight(l: ControlsLayout): number {
  const rail = l.shortcutRows > 0 ? railHeight(l.shortcutRows, SHORTCUT_HEIGHT, l.shortcutRowSpacing) : 0;
  const rows = [l.levelRow, l.cardRow, rail].filter((r) => r > 0);
  return rows.reduce((a, b) => a + b, 0) + Math.max(0, rows.length - 1) * ROW_SPACING;
}

/**
 * The levels the band can't hold, the player's card and the shortcut rail.
 * A tight budget shortens the card, then closes the rows of a rail that
 * would scroll, before it drops a row (NotchLayout.controls).
 */
export function controlsLayout(opts: {
  hasCard: boolean;
  levelRow: boolean;
  shortcuts: number;
  width: number;
  height: number;
}): ControlsLayout {
  const levels = opts.levelRow ? LEVEL_ROW_HEIGHT : 0;
  if (!opts.hasCard && opts.shortcuts === 0)
    return { levelRow: levels, cardRow: 0, shortcutRows: 0, shortcutRowSpacing: SHORTCUT_SPACING };
  const budget = opts.height - (levels > 0 ? levels + ROW_SPACING : 0);
  let rows = 0;
  let rowGap = SHORTCUT_SPACING;
  if (opts.shortcuts > 0) {
    const remaining = opts.hasCard ? budget - MINIMUM_CARD_HEIGHT - ROW_SPACING : budget;
    rows = railRows(
      opts.shortcuts,
      railCapacity(opts.width, SHORTCUT_WIDTH, SHORTCUT_SPACING),
      SHORTCUT_HEIGHT,
      SHORTCUT_SPACING,
      remaining,
    );
    // A rail that would scroll sideways closes its rows to take the row it is a few points short of.
    const scrolls =
      railGap(
        railColumns(opts.shortcuts, rows),
        SHORTCUT_WIDTH,
        SHORTCUT_SPACING,
        SHORTCUT_LEAST_SPACING,
        opts.width,
      ) === null;
    const gap = scrolls
      ? railGap(rows + 1, SHORTCUT_HEIGHT, SHORTCUT_SPACING, SHORTCUT_LEAST_ROW_SPACING, remaining)
      : null;
    if (gap !== null) {
      rows += 1;
      rowGap = gap;
    }
  }
  const rail = rows > 0 ? railHeight(rows, SHORTCUT_HEIGHT, rowGap) + ROW_SPACING : 0;
  const cardRow = opts.hasCard ? Math.min(CARD_HEIGHT, Math.max(MINIMUM_CARD_HEIGHT, budget - rail)) : 0;
  return { levelRow: levels, cardRow, shortcutRows: rows, shortcutRowSpacing: rowGap };
}

/**
 * Whether the band holds every level, each at its narrowest after the
 * sections button: all or nothing (NotchLayout.bandLevelCount, no camera).
 */
export function levelsFitBand(levels: number, leadingRoom: number): boolean {
  if (levels <= 0) return true;
  const held = levels * BAND_LEVEL_MINIMUM_WIDTH + Math.max(0, levels - 1) * BAND_LEVEL_SPACING;
  return held + BAND_LEVEL_SPACING <= leadingRoom;
}
