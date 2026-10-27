// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The shelf as the page sees it (ShelfService.swift, ShelfSupport.swift):
// which tiles show (open piles spill their items after them), the
// selection (click toggles, Shift+click adds the range from the last tile
// touched), and what each tile's tooltip says.

import { S } from "./strings";
import type { ShelfItem } from "./types";

export interface Selection {
  ids: ReadonlySet<string>;
  /** The last tile touched, where a Shift+click range starts. */
  anchor: string | null;
}

export const NO_SELECTION: Selection = { ids: new Set(), anchor: null };

/** Tiles in order: each item, then an open pile's items (ShelfService.visibleItems). */
export function visibleItems(items: readonly ShelfItem[], expanded: ReadonlySet<string>): ShelfItem[] {
  const out: ShelfItem[] = [];
  for (const item of items) {
    out.push(item);
    if (item.kind === "batch" && expanded.has(item.id)) out.push(...visibleItems(item.children ?? [], expanded));
  }
  return out;
}

/** A click: in or out of the selection, and it becomes the anchor. */
export function toggled(selection: Selection, id: string): Selection {
  const ids = new Set(selection.ids);
  if (ids.has(id)) ids.delete(id);
  else ids.add(id);
  return { ids, anchor: id };
}

/**
 * Shift+click: adds the visible tiles from the anchor to `id`, in either
 * direction, keeping what was already chosen (ShelfSelectionSupport.rangeSelectionIDs).
 */
export function extended(selection: Selection, visibleIds: readonly string[], id: string): Selection {
  const target = visibleIds.indexOf(id);
  if (target < 0) return selection;
  const anchorAt = selection.anchor === null ? -1 : visibleIds.indexOf(selection.anchor);
  const from = anchorAt < 0 ? target : anchorAt;
  const ids = new Set(selection.ids);
  for (const each of visibleIds.slice(Math.min(from, target), Math.max(from, target) + 1)) ids.add(each);
  return { ids, anchor: id };
}

export function findItem(items: readonly ShelfItem[], id: string): ShelfItem | undefined {
  for (const item of items) {
    if (item.id === id) return item;
    const inside = findItem(item.children ?? [], id);
    if (inside) return inside;
  }
  return undefined;
}

/** Every id in these items, piles' items included. */
export function allIds(items: readonly ShelfItem[], out = new Set<string>()): Set<string> {
  for (const item of items) {
    out.add(item.id);
    allIds(item.children ?? [], out);
  }
  return out;
}

/**
 * Opens or closes a pile. Closing it deselects what it held, since those
 * tiles are no longer there to see (ShelfService.toggleBatchExpansion).
 */
export function toggledPile(
  items: readonly ShelfItem[],
  expanded: ReadonlySet<string>,
  selection: Selection,
  id: string,
): { expanded: Set<string>; selection: Selection } {
  const pile = findItem(items, id);
  const open = new Set(expanded);
  if (!pile || pile.kind !== "batch") return { expanded: open, selection };
  if (!open.has(id)) {
    open.add(id);
    return { expanded: open, selection };
  }
  open.delete(id);
  const inside = allIds(pile.children ?? []);
  const ids = new Set([...selection.ids].filter((each) => !inside.has(each)));
  const anchor = selection.anchor !== null && inside.has(selection.anchor) ? null : selection.anchor;
  return { expanded: open, selection: { ids, anchor } };
}

/** Selection and open piles after the items changed: only what still exists. */
export function pruned(
  items: readonly ShelfItem[],
  expanded: ReadonlySet<string>,
  selection: Selection,
): { expanded: Set<string>; selection: Selection } {
  const alive = allIds(items);
  const piles = new Set([...alive].filter((id) => findItem(items, id)?.kind === "batch"));
  return {
    expanded: new Set([...expanded].filter((id) => piles.has(id))),
    selection: {
      ids: new Set([...selection.ids].filter((id) => alive.has(id))),
      anchor: selection.anchor !== null && alive.has(selection.anchor) ? selection.anchor : null,
    },
  };
}

/** Whether any of these, or anything in them, is a file (ShelfService.Item.holdsFile). */
export function holdsFile(items: readonly ShelfItem[]): boolean {
  return items.some((item) => item.kind === "file" || holdsFile(item.children ?? []));
}

/** Selected tiles in shelf order, piles' items included. */
export function selectedItems(items: readonly ShelfItem[], selection: ReadonlySet<string>): ShelfItem[] {
  const out: ShelfItem[] = [];
  for (const item of items) {
    if (selection.has(item.id)) out.push(item);
    out.push(...selectedItems(item.children ?? [], selection));
  }
  return out;
}

/** What the band's buttons act on: the selection, or everything when none. */
export function bandTargets(items: readonly ShelfItem[], selection: ReadonlySet<string>): ShelfItem[] {
  return selection.size ? selectedItems(items, selection) : [...items];
}

/** What a tile's menu or drag acts on: the selection when it holds the tile. */
export function tileTargets(items: readonly ShelfItem[], selection: ReadonlySet<string>, id: string): string[] {
  return selection.has(id) ? selectedItems(items, selection).map((item) => item.id) : [id];
}

const IMAGE = /\.(png|jpe?g|gif|webp|bmp|tiff?|heic|heif|avif|ico|svg)$/i;

export const isImage = (item: ShelfItem) => item.kind === "file" && IMAGE.test(item.path ?? "");

function leafCounts(item: ShelfItem, counts = { images: 0, files: 0, notes: 0, links: 0 }) {
  if (item.kind === "batch") for (const child of item.children ?? []) leafCounts(child, counts);
  else if (item.kind === "text") counts.notes++;
  else if (item.kind === "link") counts.links++;
  else if (isImage(item)) counts.images++;
  else counts.files++;
  return counts;
}

export function leafCount(item: ShelfItem): number {
  const c = leafCounts(item);
  return c.images + c.files + c.notes + c.links;
}

const TEXT_CAP = 500;
const capped = (text: string) => (text.length > TEXT_CAP ? `${text.slice(0, TEXT_CAP)}…` : text);

/**
 * The hover tooltip: a file's name and kind, the text itself, the whole
 * link, or what a pile holds (ShelfTooltipSupport).
 */
export function tooltipText(item: ShelfItem, kind?: string | null): string {
  switch (item.kind) {
    case "file":
      return kind ? `${item.title}\n${kind}` : item.title;
    case "text":
      return capped((item.text ?? "").trim());
    case "link":
      return capped(item.url ?? "");
    case "batch":
      return S.pile(leafCounts(item));
  }
}
