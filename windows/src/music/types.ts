// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust reader of Windows' media controls sends (event "music://now-playing").

export interface Playback {
  source: string;
  sourceName: string;
  isMusicApp: boolean;
  title: string;
  artist: string;
  album: string;
  artwork?: string;
  isPlaying: boolean;
  /** Seconds as of `sampledAt` (epoch seconds). */
  elapsed: number;
  duration: number;
  rate: number;
  sampledAt: number;
  canPlayPause: boolean;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
  canShuffle: boolean;
  shuffle: boolean;
}

export interface Source {
  id: string;
  name: string;
  isPlaying: boolean;
  isMusicApp?: boolean;
  hasTrack?: boolean;
}

export interface NowPlaying {
  selectedSource?: string | null;
  revision?: number;
  idlePlayer?: Source | null;
  playback?: Playback | null;
  sources: Source[];
}

export type MusicCommand = "toggle" | "next" | "previous";

/** The extrapolated position (NotchPlayback.position). */
export function position(p: Playback, now = Date.now() / 1000): number {
  const moved = p.isPlaying ? (now - p.sampledAt) * p.rate : 0;
  return Math.min(p.duration, Math.max(0, p.elapsed + moved));
}

/** m:ss, or h:mm:ss from an hour. */
export function time(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}
