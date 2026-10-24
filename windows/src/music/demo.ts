// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A song for designing the Music section in a plain browser (?music).

import type { NowPlaying } from "./types";

function cover(): string {
  const c = document.createElement("canvas");
  c.width = c.height = 160;
  const g = c.getContext("2d")!;
  const grad = g.createLinearGradient(0, 0, 160, 160);
  grad.addColorStop(0, "#ff5f6d");
  grad.addColorStop(1, "#7b2ff7");
  g.fillStyle = grad;
  g.fillRect(0, 0, 160, 160);
  g.fillStyle = "rgba(255,255,255,0.85)";
  g.beginPath();
  g.arc(80, 80, 34, 0, Math.PI * 2);
  g.fill();
  return c.toDataURL("image/png");
}

export function demoNowPlaying(): NowPlaying {
  return {
    playback: {
      source: "Spotify.exe",
      sourceName: "Spotify",
      isMusicApp: true,
      title: "Midnight City",
      artist: "M83",
      album: "Hurry Up, We're Dreaming",
      artwork: cover(),
      isPlaying: true,
      elapsed: 74,
      duration: 243,
      rate: 1,
      sampledAt: Date.now() / 1000,
      canPlayPause: true,
      canNext: true,
      canPrevious: true,
      canSeek: true,
      canShuffle: true,
      shuffle: false,
    },
    sources: [{ id: "Spotify.exe", name: "Spotify", isPlaying: true }],
  };
}
