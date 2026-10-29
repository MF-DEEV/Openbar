// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The mixer's pinned apps and their order (MixerAppArrangement,
// MixerRoutingSupport.swift:947-1013), and which apps the desk lists.

import type { AudioApp } from "../audio/types";

export interface Arrangement {
  order: string[];
  pinned: string[];
}

const unique = (ids: readonly string[]) => [...new Set(ids.filter((id) => id.length > 0))];

export const isPinned = (a: Arrangement, id: string) => a.pinned.includes(id);

/** Pinned first, then by remembered position; unknown ones keep their place after. */
export function ordered<T>(a: Arrangement, items: readonly T[], identity: (item: T) => string): T[] {
  const ranks = new Map(a.order.map((id, i) => [id, i]));
  return items
    .map((item, offset) => ({ item, offset, id: identity(item) }))
    .sort((l, r) => {
      const lp = isPinned(a, l.id);
      const rp = isPinned(a, r.id);
      if (lp !== rp) return lp ? -1 : 1;
      const lr = ranks.get(l.id) ?? Number.MAX_SAFE_INTEGER;
      const rr = ranks.get(r.id) ?? Number.MAX_SAFE_INTEGER;
      return lr === rr ? l.offset - r.offset : lr - rr;
    })
    .map((e) => e.item);
}

export function togglePin(a: Arrangement, id: string): Arrangement {
  if (!id) return a;
  return isPinned(a, id) ? { ...a, pinned: a.pinned.filter((p) => p !== id) } : { ...a, pinned: [...a.pinned, id] };
}

/** The visible neighbour `offset` away inside the same pin group. */
export function neighbor(a: Arrangement, id: string, offset: number, visible: readonly string[]): string | null {
  const group = visible.filter((v) => isPinned(a, v) === isPinned(a, id));
  const index = group.indexOf(id);
  if (index < 0) return null;
  return group[index + offset] ?? null;
}

/**
 * Moves `id` beside `target` within its pin group. Only that group's visible
 * slots change: closed and hidden apps, and the other group, keep their
 * remembered positions.
 */
export function move(
  a: Arrangement,
  id: string,
  target: string,
  after: boolean,
  visible: readonly string[],
): Arrangement {
  if (id === target || !visible.includes(id) || !visible.includes(target)) return a;
  if (isPinned(a, id) !== isPinned(a, target)) return a;
  const group = unique(visible.filter((v) => isPinned(a, v) === isPinned(a, id))).filter((v) => v !== id);
  const index = group.indexOf(target);
  if (index < 0) return a;
  group.splice(index + (after ? 1 : 0), 0, id);
  const moving = new Set(group);
  let next = 0;
  const order = unique([...a.order, ...visible]).map((v) => (moving.has(v) ? group[next++]! : v));
  return { ...a, order };
}

export function moveBy(a: Arrangement, id: string, offset: number, visible: readonly string[]): Arrangement {
  const other = neighbor(a, id, offset, visible);
  return other ? move(a, id, other, offset > 0, visible) : a;
}

/** 100%, within half a percent (MixerRoutingSupport.isUnity). */
export const isUnity = (volume: number) => Math.abs(volume - 1) < 0.005;

/**
 * The apps the desk lists, in order: none hidden from the list, and with
 * "hide inactive" on only those playing, muted or set off 100%, so hiding idle
 * rows never conceals a setting (MixerRoutingSupport.shouldShowApp).
 */
export function listedApps(
  apps: readonly AudioApp[],
  a: Arrangement,
  hidden: Readonly<Record<string, string>>,
  hideInactive: boolean,
): AudioApp[] {
  return ordered(
    a,
    apps.filter((app) => !(app.key in hidden) && (!hideInactive || app.playing || !isUnity(app.volume) || app.muted)),
    (app) => app.key,
  );
}

/** Takes a reordered list of the visible apps into the remembered order. */
export function arrange(a: Arrangement, visible: readonly string[], next: readonly string[]): Arrangement {
  const moving = new Set(visible);
  let i = 0;
  const order = unique([...a.order, ...visible]).map((v) => (moving.has(v) ? (next[i++] ?? v) : v));
  return { ...a, order };
}

/** The output switcher's cycle: chosen ids in the outputs' own order, chosen ones not connected after them. */
export function cycleOrder(chosen: string[], outputs: { id: string }[]): string[] {
  const listed = outputs.map((d) => d.id);
  return [...listed.filter((id) => chosen.includes(id)), ...chosen.filter((id) => !listed.includes(id))];
}
