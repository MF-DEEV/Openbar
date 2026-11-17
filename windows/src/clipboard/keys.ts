// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Clipboard page's keys and the list's movement (NotchSupport.swift:94-108,
// 1777-1851): Up and Down move the highlight without wrapping, Enter pastes,
// Ctrl+1–9 paste by place, and typing goes to the search field.

export type ClipboardKey = { kind: "move"; backwards: boolean } | { kind: "paste" } | { kind: "reveal" };

/** The keydown fields the model reads. */
export interface KeyInput {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  isComposing: boolean;
}

/**
 * What a key does while the page's search field keeps the focus. Keys held
 * with Ctrl, Alt or Win belong to their own shortcuts; Shift with the arrows
 * or Enter belongs to the field, which selects with them. An input method
 * that is composing gets every key.
 */
export function clipboardKey(e: KeyInput): ClipboardKey | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  if (e.isComposing || e.key === "Process") return { kind: "reveal" };
  if (e.key === "ArrowUp" || e.key === "ArrowDown")
    return e.shiftKey ? null : { kind: "move", backwards: e.key === "ArrowUp" };
  if (e.key === "Enter") return e.shiftKey ? null : { kind: "paste" };
  // A printable character: one character, not a space (dead keys report "Dead").
  const printable = Array.from(e.key).length === 1 && e.key.trim() !== "";
  return printable || e.key === "Dead" ? { kind: "reveal" } : null;
}

/**
 * Ctrl+1 to Ctrl+9 by physical key, so every layout keeps the shortcut, and
 * the numeric keypad's digits: the visible place 0–8, or null.
 */
export function pastePlace(e: KeyInput): number | null {
  if (!e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return null;
  const match = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
  return match ? Number(match[1]) - 1 : null;
}

/** The arrows step without wrapping; the first press, or one after the row left the list, lands on the top. */
export function steppedItem<T>(current: T | null, ids: T[], backwards: boolean): T | null {
  if (ids.length === 0) return null;
  const index = current === null ? -1 : ids.indexOf(current);
  if (index < 0) return ids[0]!;
  return ids[Math.min(Math.max(index + (backwards ? -1 : 1), 0), ids.length - 1)]!;
}

/** Enter pastes the highlight, or the first visible entry when the highlight is stale. */
export function pasteTarget<T>(highlighted: T | null, ids: T[]): T | null {
  if (highlighted !== null && ids.includes(highlighted)) return highlighted;
  return ids[0] ?? null;
}

/**
 * The row a search leaves highlighted: the current one while it is still
 * listed, else a typed search's top result; an empty search waits for an arrow.
 */
export function searchHighlight<T>(current: T | null, ids: T[], query: string): T | null {
  if (current !== null && ids.includes(current)) return current;
  return query.trim() === "" ? null : (ids[0] ?? null);
}

/** Opening highlights the entry copied last: the first unpinned one, or the top when all are pinned. */
export function restingHighlight<T>(entries: { id: T; pinned: boolean }[]): T | null {
  return (entries.find((e) => !e.pinned) ?? entries[0])?.id ?? null;
}

/** Search first, then the resting highlight (NotchClipboardView.searchHighlight). */
export function highlightFor<T>(current: T | null, entries: { id: T; pinned: boolean }[], query: string): T | null {
  return (
    searchHighlight(
      current,
      entries.map((e) => e.id),
      query,
    ) ?? restingHighlight(entries)
  );
}

/**
 * How far an open entry, drawn from its row's top, moves so all of it shows:
 * up just enough at the bottom, down where its row is partly scrolled away.
 * It always covers the row's visible part, and once the row has left the
 * list it scrolls away with it.
 */
export function openOffset(rowTop: number, rowHeight: number, height: number, visibleHeight: number): number {
  return Math.min(rowHeight, Math.max(-height, Math.max(-rowTop, Math.min(0, visibleHeight - rowTop - height))));
}

/**
 * Whole rows a list at its top scrolls to bring row `index` into view: none
 * while `shown` points of it show, else the fewest that show that much.
 */
export function openingScroll(
  index: number,
  rowHeight: number,
  spacing: number,
  shown: number,
  visibleHeight: number,
): number {
  const lowest = Math.floor((visibleHeight - Math.min(shown, rowHeight)) / (rowHeight + spacing));
  return Math.min(Math.max(0, index), Math.max(0, index - lowest));
}
