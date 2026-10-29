// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Playback } from "./types";
import { position } from "./types";

export interface LyricLine {
  time: number;
  text: string;
}
export interface Lyrics {
  lines: LyricLine[];
  plain: string;
  instrumental: boolean;
}
export type Song = Pick<Playback, "title" | "artist" | "album" | "duration">;
export const MAX_LRC_BYTES = 128 * 1024;
const bytes = (s: string) => new TextEncoder().encode(s).length;

/** Recording identity deliberately excludes player and playback position. */
export function songKey(p: Song): string {
  return JSON.stringify([p.title.trim(), p.artist.trim(), p.album.trim(), Math.round(p.duration || 0)]);
}

/** Same bounds, offset sign, blank verses and duplicate-time grouping as macOS. */
export function parseLrc(source: string, duration = 604800): LyricLine[] {
  if (bytes(source) > MAX_LRC_BYTES || !Number.isFinite(duration) || duration <= 0) return [];
  const entries: LyricLine[] = [];
  let offset = 0,
    expanded = 0;
  for (const raw of source.split(/\r\n|[\n\r]/)) {
    let text = raw.trim();
    const metadata = /^\[offset:\s*([+-]?\d+(?:\.\d+)?)\s*\]$/i.exec(text);
    if (metadata) {
      const value = Number(metadata[1]);
      if (Number.isFinite(value) && Math.abs(value) <= 60000) offset = value / 1000;
      continue;
    }
    const times: number[] = [];
    let match: RegExpExecArray | null;
    while ((match = /^\[(\d+):(\d+(?:\.\d+)?)\]/.exec(text))) {
      const minutes = Number(match[1]),
        seconds = Number(match[2]);
      if (minutes > 10080 || seconds >= 60) break;
      if (times.length >= 2000) return [];
      times.push(minutes * 60 + seconds);
      text = text.slice(match[0].length);
    }
    text = text.trim();
    for (const time of times) {
      expanded += bytes(text) + (text ? 1 : 0);
      if (entries.length >= 2000 || expanded > MAX_LRC_BYTES) return [];
      entries.push({ time, text });
    }
  }
  const sorted = entries
    .map((line) => ({ ...line, time: Math.max(0, line.time - offset) }))
    .filter((line) => line.time <= duration)
    .sort((a, b) => a.time - b.time);
  const lines: LyricLine[] = [];
  for (const line of sorted) {
    const last = lines.at(-1);
    if (last?.time === line.time) last.text = [last.text, line.text].filter(Boolean).join("\n");
    else lines.push({ ...line });
  }
  return lines.some((line) => line.text) ? lines : [];
}

export function activeIndex(lines: LyricLine[], time: number, offset = 0): number {
  if (!Number.isFinite(time) || !Number.isFinite(offset)) return -1;
  let lower = 0,
    upper = lines.length;
  while (lower < upper) {
    const middle = lower + Math.floor((upper - lower) / 2);
    if (lines[middle].time <= time - offset) lower = middle + 1;
    else upper = middle;
  }
  return lower - 1;
}

/** One wakeup at the next verse, no ticking clock while paused. */
export function nextLyricDelay(
  lines: LyricLine[],
  p: Playback,
  offset: number,
  now = Date.now() / 1000,
): number | null {
  if (!p.isPlaying || !Number.isFinite(p.rate) || p.rate <= 0 || p.duration <= 0) return null;
  const time = position(p, now);
  const next = lines[activeIndex(lines, time, offset) + 1];
  if (!next || next.time + offset > p.duration) return null;
  return Math.max(1, Math.ceil(((next.time + offset - time) / p.rate) * 1000));
}

const key = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]/gu, "");

// Release names may differ, but a live/remix/acoustic album must not silently
// supply a different recording just because its title and duration are close.
// The same words as macOS (NotchLyricsSupport.recordingKinds).
const KINDS = [
  "live",
  "concert",
  "концерт",
  "концертная",
  "remix",
  "remixes",
  "ремикс",
  "acoustic",
  "акустика",
  "instrumental",
  "karaoke",
];
const recordingKinds = (album: string) => {
  const words = new Set(
    album
      .normalize("NFKD")
      .replace(/\p{M}/gu, "")
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u),
  );
  return KINDS.filter((kind) => words.has(kind)).join(",");
};

export function catalogLyrics(value: unknown, song: Song): Lyrics | null {
  if (!value || typeof value !== "object") return null;
  const r = value as Record<string, unknown>;
  if (
    typeof r.trackName !== "string" ||
    !key(song.title) ||
    key(r.trackName) !== key(song.title) ||
    typeof r.artistName !== "string" ||
    key(r.artistName) !== key(song.artist) ||
    (typeof r.albumName === "string" && recordingKinds(r.albumName) !== recordingKinds(song.album)) ||
    (song.duration > 0 &&
      (typeof r.duration !== "number" || !Number.isFinite(r.duration) || Math.abs(r.duration - song.duration) > 2))
  )
    return null;
  const plain = typeof r.plainLyrics === "string" ? r.plainLyrics.trim() : "";
  const synced = typeof r.syncedLyrics === "string" ? r.syncedLyrics : "";
  if (bytes(plain) > MAX_LRC_BYTES || bytes(synced) > MAX_LRC_BYTES) return null;
  const instrumental = r.instrumental === true;
  let lines = parseLrc(synced);
  if (song.duration > 0 && (lines.at(-1)?.time ?? 0) > song.duration + 2) lines = [];
  else if (song.duration > 0) lines = lines.filter((line) => line.time <= song.duration);
  return instrumental
    ? { lines: [], plain: "", instrumental }
    : lines.length || plain
      ? { lines, plain, instrumental }
      : null;
}

class LyricsHttpError extends Error {
  constructor(readonly status: number) {
    super(`LRCLIB: ${status}`);
  }
}

async function request(path: string, query: URLSearchParams, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(`https://lrclib.net/api/${path}?${query}`, { signal });
  if (response.status === 404) return null;
  if (!response.ok) throw new LyricsHttpError(response.status);
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Empty LRCLIB response");
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 1024 * 1024) throw new Error("LRCLIB response too large");
      chunks.push(value);
    }
  } finally {
    await reader.cancel();
  }
  return JSON.parse(await new Blob(chunks as BlobPart[]).text());
}

export async function fetchLyrics(song: Song, signal: AbortSignal): Promise<Lyrics | null> {
  if (!song.title.trim() || !song.artist.trim() || [song.title, song.artist, song.album].some((s) => bytes(s) > 1024))
    return null;
  const query = new URLSearchParams({ track_name: song.title, artist_name: song.artist });
  if (song.album.trim()) query.set("album_name", song.album);
  if (Number.isFinite(song.duration) && song.duration > 0) query.set("duration", String(Math.round(song.duration)));
  let exact: Lyrics | null = null;
  try {
    const result = await request("get", query, signal);
    const album = result && typeof result === "object" ? (result as Record<string, unknown>).albumName : null;
    if (!song.album.trim() || (typeof album === "string" && key(album) === key(song.album)))
      exact = catalogLyrics(result, song);
  } catch (error) {
    // Like macOS, a broken lookup endpoint can still have working search.
    // Network errors and rate limits never fan out into further requests.
    if (!(error instanceof LyricsHttpError) || error.status < 500) throw error;
  }
  if (exact?.lines.length || exact?.instrumental) return exact;
  query.delete("duration");
  query.delete("album_name");
  let result: unknown;
  try {
    result = await request("search", query, signal);
  } catch (error) {
    if (exact && !signal.aborted) return exact;
    throw error;
  }
  const candidates = (Array.isArray(result) ? result.slice(0, 20) : [])
    .map((r) => ({
      lyrics: catalogLyrics(r, song),
      album: typeof r?.albumName === "string" && key(r.albumName) === key(song.album),
    }))
    .filter((c) => c.lyrics)
    .sort(
      (a, b) =>
        Number(!!b.lyrics?.lines.length) - Number(!!a.lyrics?.lines.length) || Number(b.album) - Number(a.album),
    );
  return candidates[0]?.lyrics ?? exact;
}
