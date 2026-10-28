// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { create } from "zustand";
import { fetchLyrics, parseLrc, songKey, type Lyrics, type Song } from "./lyrics";

interface SavedSong {
  lrc?: string;
  offset: number;
}
type StorageAccess = Pick<Storage, "getItem" | "setItem">;
const STORAGE_KEY = "vorssaint.music.lyrics.v1";

interface LyricsState {
  key: string | null;
  lyrics: Lyrics | null;
  offset: number;
  status: "idle" | "loading" | "ready" | "unavailable" | "failed";
  error: string | null;
  expanded: boolean;
  toggle(): void;
  select(song: Song | null, retry?: boolean): Promise<void>;
  importLrc(source: string, expectedKey: string): boolean;
  adjustOffset(amount: number): void;
}

/** A single request owner survives page hiding and rejects every older generation. */
export function createLyricsStore(load = fetchLyrics, storage?: StorageAccess) {
  let saved: Record<string, SavedSong> = {};
  try {
    const parsed: unknown = JSON.parse(storage?.getItem(STORAGE_KEY) ?? "{}");
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      for (const [key, value] of Object.entries(parsed).slice(-100)) {
        if (!value || typeof value !== "object") continue;
        const { offset, lrc } = value;
        if (
          typeof offset === "number" &&
          Number.isFinite(offset) &&
          Math.abs(offset) <= 10 &&
          (lrc === undefined || (typeof lrc === "string" && parseLrc(lrc).length > 0))
        )
          saved[key] = { offset, lrc };
      }
    }
  } catch {
    /* Corrupt or unavailable storage must not prevent playback. */
  }
  const cache = new Map<string, { lyrics: Lyrics | null; expires: number; failed?: boolean }>();
  let generation = 0;
  let controller: AbortController | undefined;
  let selected: Song | null = null;
  function persist(key: string, value: SavedSong): boolean {
    delete saved[key];
    saved[key] = value;
    const entries = Object.entries(saved).slice(-100);
    saved = Object.fromEntries(entries);
    try {
      storage?.setItem(STORAGE_KEY, JSON.stringify(saved));
      return !!storage;
    } catch {
      return false;
    }
  }
  return create<LyricsState>()((set, get) => ({
    key: null,
    lyrics: null,
    offset: 0,
    status: "idle",
    error: null,
    expanded: false,
    toggle: () => set({ expanded: !get().expanded }),
    async select(song, retry = false) {
      const key = song ? songKey(song) : null;
      if (key === get().key && !retry) return;
      selected = song ? { ...song } : null;
      const request = ++generation;
      controller?.abort();
      set({ key, lyrics: null, offset: key ? (saved[key]?.offset ?? 0) : 0, status: "idle", error: null });
      if (!key || !song) return;
      const imported = saved[key]?.lrc;
      if (imported) {
        set({ lyrics: { lines: parseLrc(imported), plain: "", instrumental: false }, status: "ready" });
        return;
      }
      const hit = cache.get(key);
      if (!retry && hit && hit.expires > Date.now()) {
        set({ lyrics: hit.lyrics, status: hit.failed ? "failed" : hit.lyrics ? "ready" : "unavailable" });
        return;
      }
      controller = new AbortController();
      const current = controller;
      const timeout = setTimeout(() => current.abort(), 15000);
      set({ status: "loading" });
      try {
        const lyrics = await load(song, current.signal);
        if (request !== generation) return;
        cache.delete(key);
        cache.set(key, { lyrics, expires: Date.now() + (lyrics ? 86400000 : 60000) });
        if (cache.size > 50) cache.delete(cache.keys().next().value!);
        set({ lyrics, status: lyrics ? "ready" : "unavailable" });
      } catch {
        if (request === generation) {
          cache.set(key, { lyrics: null, failed: true, expires: Date.now() + 15000 });
          if (cache.size > 50) cache.delete(cache.keys().next().value!);
          set({ status: "failed" });
        }
      } finally {
        clearTimeout(timeout);
      }
    },
    importLrc(source, expectedKey) {
      if (get().key !== expectedKey || !selected) return false;
      const lines = parseLrc(source);
      if (!lines.length) {
        set({ error: "This file has no valid timed lyrics (maximum 128 KB)." });
        return false;
      }
      ++generation;
      controller?.abort();
      const stored = persist(expectedKey, { lrc: source, offset: get().offset });
      set({
        lyrics: { lines, plain: "", instrumental: false },
        status: "ready",
        error: stored ? null : "Lyrics loaded, but could not be saved on this device.",
      });
      return true;
    },
    adjustOffset(amount) {
      const { key, offset } = get();
      if (!key || !Number.isFinite(amount)) return;
      const next = Math.min(10, Math.max(-10, Math.round((offset + amount) * 4) / 4));
      const stored = persist(key, { ...saved[key], offset: next });
      set({ offset: next, error: stored ? null : "Timing changed, but could not be saved on this device." });
    },
  }));
}

function localStorageAccess(): StorageAccess | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}
export const useLyrics = createLyricsStore(fetchLyrics, localStorageAccess());
