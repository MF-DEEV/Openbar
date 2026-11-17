// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The history the Rust side keeps, and the page state the island's top row
// shares with the page: the pinned filter (which outlives page switches, as
// the Mac keeps it on the service), the search field and the Clear sheet.

import { create } from "zustand";
import { modulesOf, useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { C } from "./strings";
import type { ClipboardEntry, ClipboardSnapshot } from "./types";

export interface ClipboardState {
  entries: ClipboardEntry[];
  pinnedOnly: boolean;
  /** The search field shows once it is wanted: the magnifier or typing opens it. */
  searchOpen: boolean;
  /** Bumped by the magnifier, so the page focuses its field. */
  searchRequest: number;
  /** The unpinned entries a Clear confirmation counted, while it is open. */
  confirmClear: Record<string, number> | null;
  receive(snapshot: ClipboardSnapshot): void;
  togglePinnedOnly(): void;
  requestSearch(): void;
  setSearchOpen(open: boolean): void;
  askClearRecent(): void;
  answerClearRecent(clear: boolean): void;
}

export const useClipboard = create<ClipboardState>()((set, get) => ({
  entries: [],
  pinnedOnly: false,
  searchOpen: false,
  searchRequest: 0,
  confirmClear: null,
  receive: (snapshot) => set({ entries: snapshot.entries }),
  togglePinnedOnly: () => set((s) => ({ pinnedOnly: !s.pinnedOnly })),
  requestSearch: () => set((s) => ({ searchOpen: true, searchRequest: s.searchRequest + 1 })),
  setSearchOpen: (searchOpen) => set({ searchOpen }),
  askClearRecent() {
    const recent = get().entries.filter((e) => !e.pinned);
    if (recent.length === 0) return;
    set({ confirmClear: Object.fromEntries(recent.map((e) => [e.id, e.copiedAt])) });
  },
  answerClearRecent(clear) {
    const confirmed = get().confirmClear;
    set({ confirmClear: null });
    if (clear && confirmed) void bridge.clipboard.clearRecent(confirmed);
  },
}));

/** A 14 pt clipboard glyph for the notice (Lucide's ClipboardList, white). */
const NOTICE_GLYPH =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="8" height="4" x="8" y="2" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><path d="M12 11h4M12 16h4M8 11h.01M8 16h.01"/></svg>',
  );

/**
 * "Copied" for a moment while the island is closed (NotchService.swift:
 * 4288-4295): priority 0, 2.5 s, only while the section shows and its notice
 * is on (NotchSupport.swift:1690-1712, 2117-2119).
 */
function announce() {
  const island = useIsland.getState();
  const { settings } = island;
  if (island.expanded || !settings.enabled || !settings.clipboardNotice) return;
  if (!settings.clipboardHistoryEnabled || !modulesOf(island).includes("clipboard")) return;
  island.showNotice({
    id: `clipboard:${Date.now()}`,
    kind: "clipboard",
    title: C.copied,
    detail: C.title,
    priority: 0,
    duration: 2.5,
    opens: "clipboard",
    image: NOTICE_GLYPH,
  });
}

export async function subscribeClipboard() {
  // Listen first: the first fetch must not overwrite a newer event.
  let version = 0;
  await bridge.clipboard.onHistory((snapshot) => {
    version++;
    useClipboard.getState().receive(snapshot);
    if (snapshot.captured) announce();
  });
  const at = version;
  const first = await bridge.clipboard.snapshot();
  if (first && version === at) useClipboard.getState().receive(first);
}
