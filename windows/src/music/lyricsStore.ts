// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { create } from "zustand";
import { useIsland } from "../island/store";
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
  /** consent: online lookup is off, so the panel offers it (macOS NotchLyricsService). */
  status: "idle" | "consent" | "loading" | "ready" | "unavailable" | "failed";
  error: string | null;
  expanded: boolean;
  /** The panel is on screen (LyricsPanel mounted), as macOS's visible: only then is LRCLIB asked. */
  visible: boolean;
  /** Whether LRCLIB may be asked (Content › Now Playing); off until the person allows it. */
  online: boolean;
  toggle(): void;
  setVisible(visible: boolean): void;
  setOnline(online: boolean): void;
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
    const before = saved;
    const next = { ...saved };
    delete next[key];
    next[key] = value;
    saved = Object.fromEntries(Object.entries(next).slice(-100));
    try {
      storage?.setItem(STORAGE_KEY, JSON.stringify(saved));
      return !!storage;
    } catch {
      // Keep what is stored, so one file too large to save never blocks later saves.
      saved = before;
      return false;
    }
  }
  /** Asks LRCLIB only while the panel is on screen and online lookup is allowed. */
  async function lookup(retry = false) {
    const { key, lyrics, status, visible, online } = useStore.getState();
    const song = selected;
    if (!key || !song || !visible || lyrics || (status === "loading" && !retry)) return;
    if (!online) {
      useStore.setState({ status: "consent" });
      return;
    }
    const set = useStore.setState;
    const request = ++generation;
    controller?.abort();
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
  }
  /** Hiding the panel or turning lookup off stops a request in flight. */
  function cancel() {
    const { status, visible, online, lyrics } = useStore.getState();
    if (status !== "loading" && status !== "consent") return;
    ++generation;
    controller?.abort();
    useStore.setState({ status: visible && !online && !lyrics ? "consent" : "idle" });
  }
  const useStore = create<LyricsState>()((set, get) => ({
    key: null,
    lyrics: null,
    offset: 0,
    status: "idle",
    error: null,
    expanded: false,
    visible: false,
    online: false,
    toggle: () => set({ expanded: !get().expanded }),
    setVisible(visible) {
      if (visible === get().visible) return;
      set({ visible });
      if (visible) void lookup();
      else cancel();
    },
    setOnline(online) {
      if (online === get().online) return;
      set({ online });
      if (online) void lookup();
      else cancel();
    },
    async select(song, retry = false) {
      const key = song ? songKey(song) : null;
      if (key === get().key && !retry) return;
      selected = song ? { ...song } : null;
      ++generation;
      controller?.abort();
      set({ key, lyrics: null, offset: key ? (saved[key]?.offset ?? 0) : 0, status: "idle", error: null });
      if (!key || !song) return;
      const imported = saved[key]?.lrc;
      if (imported) {
        set({ lyrics: { lines: parseLrc(imported), plain: "", instrumental: false }, status: "ready" });
        return;
      }
      await lookup(retry);
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
  return useStore;
}

function localStorageAccess(): StorageAccess | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}
export const useLyrics = createLyricsStore(fetchLyrics, localStorageAccess());

// Follows the setting wherever it changes (the Settings window included),
// not only while the Music page is mounted.
useLyrics.getState().setOnline(useIsland.getState().settings.lyricsOnline);
useIsland.subscribe((state) => useLyrics.getState().setOnline(state.settings.lyricsOnline));
