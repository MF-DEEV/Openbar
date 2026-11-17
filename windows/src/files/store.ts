// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Files page's state: the shelf and ZIP job as the backend last sent
// them, and what is the page's own (the selection and which piles are open).

import { create } from "zustand";
import { allIds, extended, NO_SELECTION, pruned, type Selection, toggled, toggledPile, visibleItems } from "./model";
import type { FilesSnapshot, ShelfItem, ZipStatus } from "./types";

export const IDLE_ZIP: ZipStatus = {
  running: false,
  completed: 0,
  total: 0,
  outputs: [],
  failure: null,
  cancelled: false,
};

export interface FilesState {
  items: ShelfItem[];
  zip: ZipStatus;
  selection: Selection;
  expanded: Set<string>;
  /** Bumped when something new lands, so the tiles bring it into view. */
  arrivals: number;
  receive(snapshot: FilesSnapshot): void;
  receiveZip(zip: ZipStatus): void;
  /** A tile click; Shift extends from the last tile touched. */
  click(id: string, shift: boolean): void;
  togglePile(id: string): void;
  clearSelection(): void;
}

export const useFiles = create<FilesState>()((set, get) => ({
  items: [],
  zip: IDLE_ZIP,
  selection: NO_SELECTION,
  expanded: new Set(),
  arrivals: 0,

  receive(snapshot) {
    const s = get();
    // Ids at every depth: a pile dissolving into its last item is no arrival.
    const known = allIds(s.items);
    const arrived = snapshot.items.some((item) => !known.has(item.id));
    set({
      items: snapshot.items,
      zip: snapshot.zip,
      arrivals: arrived ? s.arrivals + 1 : s.arrivals,
      ...pruned(snapshot.items, s.expanded, s.selection),
    });
  },

  receiveZip: (zip) => set({ zip }),

  click(id, shift) {
    const s = get();
    const visible = visibleItems(s.items, s.expanded).map((item) => item.id);
    set({ selection: shift ? extended(s.selection, visible, id) : toggled(s.selection, id) });
  },

  togglePile(id) {
    const s = get();
    set(toggledPile(s.items, s.expanded, s.selection, id));
  },

  clearSelection: () => set({ selection: NO_SELECTION }),
}));
