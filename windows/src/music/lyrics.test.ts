// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
import { activeIndex, catalogLyrics, fetchLyrics, nextLyricDelay, parseLrc, songKey } from "./lyrics";
import { createLyricsStore } from "./lyricsStore";
import { position, type Playback } from "./types";

const song = { title: "Track", artist: "Artist", album: "Album", duration: 120 };
const other = { ...song, title: "Other" };
const lyrics = {
  lines: [
    { time: 1, text: "First" },
    { time: 3, text: "Second" },
    { time: 5, text: "Last" },
  ],
  plain: "",
  instrumental: false,
};
const playback: Playback = {
  ...song,
  source: "player",
  sourceName: "Player",
  isMusicApp: true,
  isPlaying: true,
  elapsed: 0,
  sampledAt: 100,
  rate: 1,
  canPlayPause: true,
  canNext: true,
  canPrevious: true,
  canSeek: true,
  canShuffle: false,
  shuffle: false,
};
const record = (overrides = {}) => ({
  trackName: song.title,
  artistName: song.artist,
  albumName: song.album,
  duration: 120,
  plainLyrics: "Plain",
  syncedLyrics: "[00:01]First",
  instrumental: false,
  ...overrides,
});
function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
}
function pending<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("LRC compatibility with macOS", () => {
  it("handles BOM, CRLF, metadata, fractions, multiple timestamps and stable grouping", () => {
    expect(
      parseLrc("\uFEFF[ar:Artist]\r\n[00:03.500][00:01.25]verse\r\n[00:01.250]translation\n[00:04]\n[offset:250]"),
    ).toEqual([
      { time: 1, text: "verse\ntranslation" },
      { time: 3.25, text: "verse" },
      { time: 3.75, text: "" },
    ]);
  });
  it("supports negative metadata offsets and clamps negative times", () => {
    expect(parseLrc("[offset:-500]\n[00:00]first")[0].time).toBe(0.5);
    expect(parseLrc("[offset:2000]\n[00:01]first")[0].time).toBe(0);
  });
  it("rejects invalid, empty, oversized and excessively expanded files", () => {
    expect(parseLrc("[00:60]invalid\n[ar:Artist]\n[00:01]")).toEqual([]);
    expect(parseLrc("[00:01]" + "x".repeat(128 * 1024))).toEqual([]);
    expect(parseLrc("[00:01]".repeat(2001) + "x")).toEqual([]);
    expect(parseLrc("[00:01]".repeat(100) + "x".repeat(2000))).toEqual([]);
    expect(parseLrc("[00:02]late", 1)).toEqual([]);
  });
  it("finds exact boundaries, blank verses, offsets and backwards seeks", () => {
    const lines = parseLrc("[00:01]first\n[00:03]\n[00:05]last");
    expect([0, 1, 2.99, 3, 5, 1].map((time) => activeIndex(lines, time))).toEqual([-1, 0, 0, 1, 2, 0]);
    expect(activeIndex(lines, 1, 0.5)).toBe(-1);
    expect(activeIndex(lines, 0.5, -0.5)).toBe(0);
    expect(activeIndex(lines, NaN)).toBe(-1);
  });
});

describe("SMTC lyric scheduling", () => {
  it("extrapolates playing time, freezes while paused, resumes and reschedules seeks", () => {
    expect(nextLyricDelay(lyrics.lines, playback, 0, 100)).toBe(1000);
    expect(position(playback, 102)).toBe(2);
    expect(activeIndex(lyrics.lines, position(playback, 102))).toBe(0);
    const paused = { ...playback, isPlaying: false, elapsed: 2, sampledAt: 102 };
    expect(position(paused, 200)).toBe(2);
    expect(nextLyricDelay(lyrics.lines, paused, 0, 200)).toBeNull();
    const resumed = { ...paused, isPlaying: true, sampledAt: 200 };
    expect(nextLyricDelay(lyrics.lines, resumed, 0, 200)).toBe(1000);
    expect(nextLyricDelay(lyrics.lines, { ...resumed, elapsed: 4 }, 0, 200)).toBe(1000);
    expect(nextLyricDelay(lyrics.lines, { ...resumed, elapsed: 0 }, 0, 200)).toBe(1000);
  });
  it("accounts for playback rate, offsets, final verse and absent timeline", () => {
    expect(nextLyricDelay(lyrics.lines, { ...playback, rate: 2 }, 0.5, 100)).toBe(750);
    expect(nextLyricDelay(lyrics.lines, { ...playback, elapsed: 5 }, 0, 100)).toBeNull();
    expect(nextLyricDelay(lyrics.lines, { ...playback, duration: 0 }, 0, 100)).toBeNull();
    expect(nextLyricDelay(lyrics.lines, { ...playback, rate: NaN }, 0, 100)).toBeNull();
  });
});

describe("LRCLIB", () => {
  it("validates recording metadata and accepts cosmetic punctuation", () => {
    expect(catalogLyrics(record({ trackName: "Track!" }), song)?.lines).toHaveLength(1);
    expect(catalogLyrics(record({ artistName: "Someone else" }), song)).toBeNull();
    expect(catalogLyrics(record({ duration: 123 }), song)).toBeNull();
    expect(catalogLyrics(record({ duration: NaN }), song)).toBeNull();
    expect(catalogLyrics(record({ albumName: "Live at the venue" }), song)).toBeNull();
    expect(catalogLyrics(record({ instrumental: true }), song)).toEqual({ lines: [], plain: "", instrumental: true });
    expect(catalogLyrics(record({ syncedLyrics: "[03:00]beyond recording" }), song)?.lines).toEqual([]);
  });
  it("sends available metadata and prefers synchronized results over plain lyrics", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(Response.json(record({ syncedLyrics: null })))
      .mockResolvedValueOnce(
        Response.json([record({ syncedLyrics: "[00:02]Synced" }), record({ syncedLyrics: null })]),
      );
    vi.stubGlobal("fetch", fetch);
    expect((await fetchLyrics(song, new AbortController().signal))?.lines[0].text).toBe("Synced");
    const url = new URL(fetch.mock.calls[0][0]);
    expect(Object.fromEntries(url.searchParams)).toEqual({
      track_name: "Track",
      artist_name: "Artist",
      album_name: "Album",
      duration: "120",
    });
  });
  it("searches after a miss, handles missing duration and does not fan out on network errors", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 404 }))
      .mockResolvedValueOnce(Response.json([record()]));
    vi.stubGlobal("fetch", fetch);
    expect(await fetchLyrics({ ...song, duration: 0, album: "" }, new AbortController().signal)).not.toBeNull();
    expect(new URL(fetch.mock.calls[0][0]).searchParams.has("duration")).toBe(false);
    fetch.mockReset().mockRejectedValue(new Error("offline"));
    await expect(fetchLyrics(song, new AbortController().signal)).rejects.toThrow("offline");
    expect(fetch).toHaveBeenCalledTimes(1);
  });
  it("preserves plain lyrics if the fallback search fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(Response.json(record({ syncedLyrics: null })))
        .mockRejectedValue(new Error("offline")),
    );
    expect((await fetchLyrics(song, new AbortController().signal))?.plain).toBe("Plain");
  });
  it("falls back after server failure, but stops on rate limits and bounds replies", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 503 }))
      .mockResolvedValueOnce(Response.json([record()]));
    vi.stubGlobal("fetch", fetch);
    expect((await fetchLyrics(song, new AbortController().signal))?.lines).toHaveLength(1);
    fetch.mockReset().mockResolvedValue(new Response(null, { status: 429 }));
    await expect(fetchLyrics(song, new AbortController().signal)).rejects.toThrow("429");
    expect(fetch).toHaveBeenCalledTimes(1);
    fetch.mockReset().mockResolvedValue(new Response("x".repeat(1024 * 1024 + 1)));
    await expect(fetchLyrics(song, new AbortController().signal)).rejects.toThrow("too large");
  });
});

/** The panel open with online lookup allowed: the only time LRCLIB is asked. */
function opened(store: ReturnType<typeof createLyricsStore>) {
  store.setState({ expanded: true, online: true });
  return store;
}

describe("lyrics lifecycle and persistence", () => {
  it("rejects stale responses after switching songs, even A to B to A", async () => {
    const a = pending<typeof lyrics | null>(),
      b = pending<typeof lyrics | null>(),
      newest = pending<typeof lyrics | null>();
    const load = vi
      .fn()
      .mockReturnValueOnce(a.promise)
      .mockReturnValueOnce(b.promise)
      .mockReturnValueOnce(newest.promise);
    const store = opened(createLyricsStore(load, memoryStorage()));
    const first = store.getState().select(song),
      second = store.getState().select(other),
      third = store.getState().select(song);
    a.resolve(lyrics);
    b.resolve(lyrics);
    await Promise.all([first, second]);
    expect(store.getState().status).toBe("loading");
    newest.resolve(null);
    await third;
    expect(store.getState().lyrics).toBeNull();
    expect(load.mock.calls[0][1].aborted).toBe(true);
  });
  it("caches positive and negative results and clears when the session disappears", async () => {
    const load = vi.fn().mockResolvedValueOnce(lyrics).mockResolvedValueOnce(null);
    const store = opened(createLyricsStore(load, memoryStorage()));
    await store.getState().select(song);
    await store.getState().select(other);
    await store.getState().select(song);
    expect(store.getState().lyrics).toEqual(lyrics);
    await store.getState().select(other);
    expect(store.getState().status).toBe("unavailable");
    expect(load).toHaveBeenCalledTimes(2);
    await store.getState().select(null);
    expect(store.getState().lyrics).toBeNull();
  });
  it("imports without allowing pending API results to replace it and persists per song", async () => {
    const storage = memoryStorage(),
      request = pending<typeof lyrics | null>();
    const store = opened(createLyricsStore(() => request.promise, storage));
    const loading = store.getState().select(song);
    expect(store.getState().importLrc("[00:02]Imported", songKey(song))).toBe(true);
    store.getState().adjustOffset(-0.5);
    request.resolve(lyrics);
    await loading;
    expect(store.getState().lyrics?.lines[0].text).toBe("Imported");
    const load = vi.fn().mockResolvedValue(null);
    const restored = opened(createLyricsStore(load, storage));
    await restored.getState().select(song);
    expect(restored.getState().offset).toBe(-0.5);
    expect(restored.getState().lyrics?.lines[0].text).toBe("Imported");
    expect(load).not.toHaveBeenCalled();
    await restored.getState().select(other);
    expect(restored.getState().offset).toBe(0);
    expect(restored.getState().importLrc("[00:02]Wrong song", songKey(song))).toBe(false);
  });
  it("keeps current lyrics on invalid imports, bounds offsets, and reports storage failure", async () => {
    const store = opened(
      createLyricsStore(async () => lyrics, {
        getItem: () => "invalid",
        setItem: () => {
          throw new Error("quota");
        },
      }),
    );
    await store.getState().select(song);
    expect(store.getState().importLrc("not timed", songKey(song))).toBe(false);
    expect(store.getState().lyrics).toEqual(lyrics);
    store.getState().adjustOffset(100);
    expect(store.getState().offset).toBe(10);
    store.getState().adjustOffset(-100);
    expect(store.getState().offset).toBe(-10);
    store.getState().adjustOffset(NaN);
    expect(store.getState().offset).toBe(-10);
    expect(store.getState().error).toContain("could not be saved");
  });
  it("handles failures, retry and request timeout without unhandled rejections", async () => {
    vi.useFakeTimers();
    const load = vi.fn().mockRejectedValueOnce(new Error("network")).mockResolvedValueOnce(lyrics);
    const store = opened(createLyricsStore(load, memoryStorage()));
    await store.getState().select(song);
    expect(store.getState().status).toBe("failed");
    await store.getState().select(song, true);
    expect(store.getState().status).toBe("ready");
    const slow = opened(
      createLyricsStore(
        (_song, signal) =>
          new Promise((_resolve, reject) => signal.addEventListener("abort", () => reject(new Error("timeout")))),
        memoryStorage(),
      ),
    );
    const loading = slow.getState().select(song);
    await vi.advanceTimersByTimeAsync(15000);
    await loading;
    expect(slow.getState().status).toBe("failed");
  });
  it("retains failed lookups briefly and expires misses so returning songs can refetch", async () => {
    vi.useFakeTimers();
    const load = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(null);
    const store = opened(createLyricsStore(load, memoryStorage()));
    await store.getState().select(song);
    await store.getState().select(other);
    await store.getState().select(song);
    expect(store.getState().status).toBe("failed");
    expect(load).toHaveBeenCalledTimes(2);
    await store.getState().select(null);
    await vi.advanceTimersByTimeAsync(15001);
    await store.getState().select(song);
    expect(store.getState().status).toBe("unavailable");
    await store.getState().select(null);
    await vi.advanceTimersByTimeAsync(60001);
    await store.getState().select(song);
    expect(load).toHaveBeenCalledTimes(4);
  });
});

describe("asking LRCLIB only when allowed (macOS notchLyricsOnline)", () => {
  it("never asks while the panel is closed, and asks once it opens", async () => {
    const load = vi.fn(async (_song: typeof song, _signal: AbortSignal) => lyrics);
    const store = createLyricsStore(load, memoryStorage());
    store.getState().setOnline(true);
    await store.getState().select(song);
    await store.getState().select(other);
    expect(load).not.toHaveBeenCalled();
    store.getState().toggle();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));
    expect(load).toHaveBeenCalledTimes(1);
    expect(load.mock.calls[0]![0]).toMatchObject({ title: "Other" });
  });

  it("offers online lookup instead of asking until it is allowed", async () => {
    const load = vi.fn(async () => lyrics);
    const store = createLyricsStore(load, memoryStorage());
    await store.getState().select(song);
    store.getState().toggle();
    expect(store.getState().status).toBe("consent");
    expect(load).not.toHaveBeenCalled();
    store.getState().setOnline(true);
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("imported lyrics need no lookup, and closing the panel stops one in flight", async () => {
    const request = pending<typeof lyrics | null>();
    let signal: AbortSignal | undefined;
    const storage = memoryStorage();
    const store = opened(
      createLyricsStore((_, s) => {
        signal = s;
        return request.promise;
      }, storage),
    );
    void store.getState().select(song);
    expect(store.getState().status).toBe("loading");
    store.getState().toggle();
    expect(signal?.aborted).toBe(true);
    expect(store.getState().status).toBe("idle");
    request.resolve(lyrics);
    await Promise.resolve();
    expect(store.getState().lyrics).toBeNull();
    store.getState().importLrc("[00:01.00]Imported", store.getState().key!);
    const load = vi.fn(async () => lyrics);
    const restored = createLyricsStore(load, storage);
    await restored.getState().select(song);
    restored.getState().toggle();
    expect(restored.getState().status).toBe("ready");
    expect(load).not.toHaveBeenCalled();
  });
});
