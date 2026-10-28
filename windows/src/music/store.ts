// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's view of what plays: the latest reading, the 1.5 s gap grace
// between songs, the optimistic play/pause flip, and the accent tint.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { tintOf } from "./tint";
import { useLyrics } from "./lyricsStore";
import type { MusicCommand, NowPlaying, Playback } from "./types";

/** A reading that finds nothing right after a playing song waits this long. */
const GAP_GRACE = 1500;
/** The requested play/pause state shows until the player confirms, at most this long. */
const OPTIMISTIC_FOR = 1500;

interface MusicState {
  playback: Playback | null;
  sources: NowPlaying["sources"];
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

export const useMusic = create<MusicState>()((set, get) => ({
  playback: null,
  sources: [],
  tint: null,
  optimistic: null,
  failed: false,

  receive(now) {
    const next = now.playback ?? null;
    const current = get().playback;
    clearTimeout(gapTimer);
    if (!next && current?.isPlaying) {
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
      optimistic: confirmed || (optimistic && Date.now() > optimistic.until) ? null : optimistic,
    });
    if (next?.artwork && next.artwork !== current?.artwork) {
      void tintOf(next.artwork).then((tint) => {
        if (get().playback?.artwork === next.artwork) set({ tint });
      });
    } else if (!next?.artwork) {
      set({ tint: null });
    }
  },

  async command(command) {
    const p = get().playback;
    if (command === "toggle" && p) {
      set({ optimistic: { playing: !p.isPlaying, until: Date.now() + OPTIMISTIC_FOR } });
      setTimeout(() => {
        const o = get().optimistic;
        if (o && Date.now() >= o.until) set({ optimistic: null });
      }, OPTIMISTIC_FOR + 50);
    }
    const ok = await bridge.music.command(command);
    set({ failed: ok === false });
  },

  async seek(seconds) {
    try {
      const ok = await bridge.music.seek(seconds);
      set({ failed: ok === false });
      return ok !== false;
    } catch {
      set({ failed: true });
      return false;
    }
  },

  async shuffle(on) {
    await bridge.music.shuffle(on);
  },
}));

/** Playing, as the person last asked or as the player says. */
export function isPlaying(s: Pick<MusicState, "playback" | "optimistic">): boolean {
  return s.optimistic?.playing ?? s.playback?.isPlaying ?? false;
}

export function subscribeMusic(): () => void {
  void bridge.music.now().then((n) => n && useMusic.getState().receive(n));
  const off = bridge.music.onNowPlaying((n) => useMusic.getState().receive(n));
  return () => void off.then((f) => f());
}
