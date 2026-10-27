// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The card grid's measurements (NotchLayout in NotchSupport.swift:319-343,
// 518-551), shared by the page and the island height it asks for.

export const CARD_HEIGHT = 72;
export const CARD_WIDTH = 128;
export const ROW_SPACING = 10;
/** NotchLayout.emptyHeight. */
export const EMPTY_HEIGHT = 140;
/** NotchLayout.pageContentHeight: the room a metric detail gets under a preset. */
export const PAGE_CONTENT_HEIGHT = 320;

/** Room for a card's hover scale, including a single full-width card. */
export function hoverInset(width: number): number {
  return Math.ceil(Math.max(0, width) * 0.015) + 1;
}

export function railCapacity(width: number, itemWidth: number, spacing: number): number {
  if (!Number.isFinite(width) || itemWidth <= 0) return 1;
  return Math.max(1, Math.floor((width + spacing) / (itemWidth + spacing)));
}

export function railHeight(rows: number, rowHeight: number, spacing: number): number {
  return Math.max(1, rows) * rowHeight + Math.max(0, rows - 1) * spacing;
}

/**
 * Balance complete rows across the width, keeping reading order and letting
 * each row fill its width without empty cells: [start, end) per row.
 */
export function systemRowRanges(count: number, width: number): [number, number][] {
  if (count <= 0) return [];
  const columns = railCapacity(width, CARD_WIDTH, ROW_SPACING);
  const rows = Math.ceil(count / columns);
  const base = Math.floor(count / rows);
  const remainder = count % rows;
  return Array.from({ length: rows }, (_, row) => {
    const start = row * base + Math.min(row, remainder);
    return [start, start + base + (row < remainder ? 1 : 0)];
  });
}

/** The grid's height on a page `width` wide, with its hover inset (NotchGeometry.expandedSize, .system). */
export function gridHeight(count: number, width: number): number {
  if (count <= 0) return EMPTY_HEIGHT;
  const inset = hoverInset(width);
  const rows = systemRowRanges(count, width - inset * 2).length;
  return railHeight(rows, CARD_HEIGHT, ROW_SPACING) + inset * 2;
}
