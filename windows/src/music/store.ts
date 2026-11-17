// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's view of what plays: the latest reading, the 1.5 s gap grace
// between songs, the optimistic play/pause flip, and the accent tint.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { tintOf } from "./tint";
import { useLyrics } from "./lyricsStore";
import type { MusicCommand, NowPlaying, Playback } from "./types";
import { loadPrefs, onPrefs } from "../shared/prefs";

/** A reading that finds nothing right after a playing song waits this long. */
const GAP_GRACE = 1500;
/** The requested play/pause state shows until the player confirms, at most this long. */
const OPTIMISTIC_FOR = 1500;

interface MusicState {
  playback: Playback | null;
  sources: NowPlaying["sources"];
  selectedSource: string | null;
  idlePlayer: NowPlaying["idlePlayer"];
  revision: number;
  selecting: boolean;
  sourceError: string | null;
  includeOtherPlayers: boolean;
  selectSource(source: string | null): Promise<void>;
  tint: string | null;
  /** The flip the person asked for, until the player says so too. */
  optimistic: { playing: boolean; until: number } | null;
  failed: boolean;
  receive(now: NowPlaying): void;
  command(command: MusicCommand): Promise<void>;
  seek(seconds: number): Promise<boolean>;
  shuffle(on: boolean): Promise<void>;
}

let gapTimer: ReturnType<typeof setTimeout> | undefined;
let pendingSelection: string | null | undefined;

export const useMusic = create<MusicState>()((set, get) => ({
  playback: null,
  sources: [],
  selectedSource: null,
  idlePlayer: null,
  revision: 0,
  selecting: false,
  sourceError: null,
  includeOtherPlayers: true,
  tint: null,
  optimistic: null,
  failed: false,

  receive(now) {
    if ((now.revision ?? 0) < get().revision) return;
    const selectedSource = now.selectedSource ?? null;
    if (pendingSelection !== undefined && pendingSelection !== selectedSource) return;
    const sourceChanged = selectedSource !== get().selectedSource || (now.revision ?? 0) !== get().revision;
    set({ selectedSource, revision: now.revision ?? 0, idlePlayer: now.idlePlayer ?? null });
    const next = now.playback ?? null;
    const current = get().playback;
    clearTimeout(gapTimer);
    if (
      !next &&
      current?.isPlaying &&
      !sourceChanged &&
      now.sources.some((s) => s.id === current.source) &&
      (current.isMusicApp || get().includeOtherPlayers || selectedSource === current.source)
    ) {
      // A gap between songs: hold the last one briefly.
      gapTimer = setTimeout(() => {
        set({ playback: null, tint: null });
        void useLyrics.getState().select(null);
      }, GAP_GRACE);
      set({ sources: now.sources });
      return;
    }
    const optimistic = get().optimistic;
    void useLyrics.getState().select(next);
    const confirmed = optimistic && next && next.isPlaying === optimistic.playing;
    set({
      playback: next,
      sources: now.sources,
      failed: sourceChanged || next?.source !== current?.source ? false : get().failed,
      optimistic:
        sourceChanged || next?.source !== current?.source || confirmed || (optimistic && Date.now() > optimistic.until)
          ? null
          : optimistic,
    });
    if (next?.artwork && next.artwork !== current?.artwork) {
      void tintOf(next.artwork).then((tint) => {
        if (get().playback?.artwork === next.artwork) set({ tint });
      });
    } else if (!next?.artwork) {
      set({ tint: null });
    }
  },

  async selectSource(source) {
    if (get().selecting || source === get().selectedSource) return;
    pendingSelection = source;
    clearTimeout(gapTimer);
    set({ selecting: true, playback: null, tint: null, optimistic: null, failed: false, sourceError: null });
    void useLyrics.getState().select(null);
    try {
      await bridge.music.selectSource(source);
    } catch {
      pendingSelection = undefined;
      set({ sourceError: "Could not change music source. Try again." });
      const now = await bridge.music.now().catch(() => null);
      if (now) get().receive(now);
    } finally {
      pendingSelection = undefined;
      set({ selecting: false });
    }
  },

  async command(command) {
    const p = get().playback;
    if (!p || get().selecting) return;
    if (command === "toggle" && p) {
      set({ optimistic: { playing: !p.isPlaying, until: Date.now() + OPTIMISTIC_FOR } });
      setTimeout(() => {
        const o = get().optimistic;
        if (o && Date.now() >= o.until) set({ optimistic: null });
      }, OPTIMISTIC_FOR + 50);
    }
    try {
      const ok = await bridge.music.command(command, p.source);
      if (get().playback?.source === p.source)
        set({ failed: ok === false, ...(ok === false ? { optimistic: null } : {}) });
    } catch {
      if (get().playback?.source === p.source) set({ failed: true, optimistic: null });
    }
  },

  async seek(seconds) {
    const source = get().playback?.source;
    if (!source || get().selecting) return false;
    try {
      const ok = await bridge.music.seek(seconds, source);
      if (get().playback?.source === source) set({ failed: ok === false });
      return ok !== false;
    } catch {
      if (get().playback?.source === source) set({ failed: true });
      return false;
    }
  },

  async shuffle(on) {
    const source = get().playback?.source;
    if (!source || get().selecting) return;
    try {
      const ok = await bridge.music.shuffle(on, source);
      if (get().playback?.source === source) set({ failed: ok === false });
    } catch {
      if (get().playback?.source === source) set({ failed: true });
    }
  },
}));

/** Playing, as the person last asked or as the player says. */
export function isPlaying(s: Pick<MusicState, "playback" | "optimistic">): boolean {
  return s.optimistic?.playing ?? s.playback?.isPlaying ?? false;
}

export function subscribeMusic(): () => void {
  let stopped = false;
  let version = 0;
  const off = bridge.music.onNowPlaying((n) => {
    if (!stopped) {
      version++;
      useMusic.getState().receive(n);
    }
  });
  void off
    .then(async () => {
      const at = version;
      const n = await bridge.music.now();
      if (!stopped && version === at && n) useMusic.getState().receive(n);
    })
    .catch(() => useMusic.setState({ failed: true }));
  const apply = (p: Awaited<ReturnType<typeof loadPrefs>>) => {
    if (stopped) return;
    useMusic.setState({ includeOtherPlayers: p.island.includeOtherPlayers });
  };
  const prefsOff = onPrefs(apply);
  void loadPrefs().then(apply);
  return () => {
    stopped = true;
    void off.then((f) => f());
    void prefsOff.then((f) => f());
  };
}
