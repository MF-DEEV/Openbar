// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Live readings for the home page's tiles from the sections that own them:
// the timer's clock (NotchTimerTile) and the next event (NotchCalendarTile).
// A section registers a hook from its own register.tsx; until one does, the
// tile is plain and opens the section's page.
//
//   registerTileReading("timer", () => {
//     const session = useTimer((s) => s.session);
//     return session ? { reading: "4:59", title: "Focus", active: true, accent: "timer" } : null;
//   });

import type { LucideIcon } from "lucide-react";
import { create } from "zustand";
import type { ControlItem, TileAccent } from "./types";

export interface TileReading {
  /** Replaces the tile's glyph (a paused timer's pause mark). */
  icon?: LucideIcon;
  /** Replaces the title, e.g. the next event's name. */
  title?: string;
  /** The first line, in rolling digits (they roll down as the number falls). */
  reading?: string;
  /** A shorter title for the line under a reading when `title` is long. */
  compactTitle?: string;
  active?: boolean;
  accent?: TileAccent;
  /** Replaces the default action (opening the section). */
  onSelect?(): void;
}

/** A React hook: called on every render of its tile; null keeps the plain tile. */
export type TileReadingHook = () => TileReading | null;

interface Registry {
  hooks: Partial<Record<ControlItem, { hook: TileReadingHook; id: number }>>;
}

let nextId = 0;

export const useTileRegistry = create<Registry>(() => ({ hooks: {} }));

/** Gives `item`'s tile a live reading; the newest registration wins. */
export function registerTileReading(item: ControlItem, hook: TileReadingHook): void {
  const id = ++nextId;
  useTileRegistry.setState((s) => ({ hooks: { ...s.hooks, [item]: { hook, id } } }));
}
