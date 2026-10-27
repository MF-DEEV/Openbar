// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample watches for designing in a plain browser: island.html?watch (an
// export at 42 %), ?watch=image (an area without text), ?watch=reading
// (before the first reading), ?watch=hidden, ?watch=denied, ?watch=done.

import { useWatch } from "./store";
import { IDLE, type WatchSnapshot } from "./types";

function picture(label: string, fraction: number | null): string {
  const bar =
    fraction === null
      ? `<rect x="16" y="30" width="328" height="44" rx="6" fill="#2b6cb0"/><rect x="40" y="44" width="120" height="16" rx="8" fill="#90cdf4"/><circle cx="300" cy="52" r="12" fill="#f6ad55"/>`
      : `<text x="16" y="34" font-family="Segoe UI, sans-serif" font-size="16" fill="#e2e8f0">${label}</text>` +
        `<rect x="16" y="50" width="328" height="10" rx="5" fill="#2d3748"/>` +
        `<rect x="16" y="50" width="${328 * fraction}" height="10" rx="5" fill="#48bb78"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="360" height="90"><rect width="360" height="90" fill="#1a202c"/>${bar}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export function startWatchDemo(kind: string | null) {
  const watching: WatchSnapshot = {
    ...IDLE,
    phase: "watching",
    target: { appName: "HandBrake", windowTitle: "Encoding: Holiday 2026.mp4", icon: null, isWindow: true },
    preview: picture("Exporting video… 42%", 0.42),
    text: "Exporting video… 42%\nTime remaining 03:12",
    headline: "42%",
    lines: ["Exporting video… 42%", "Time remaining 03:12"],
    startedAt: Date.now() - 4 * 60_000,
    condition: "reaches",
    matchNumber: 100,
  };
  const samples: Record<string, WatchSnapshot> = {
    watching,
    image: { ...watching, preview: picture("", null), text: "", headline: "", lines: [], condition: "changes" },
    reading: { ...watching, preview: null, text: "", headline: "", lines: [], condition: "changes" },
    hidden: { ...watching, phase: "hidden" },
    denied: { ...watching, phase: "hidden", accessDenied: true },
    done: {
      ...watching,
      phase: "finished",
      outcome: { kind: "reached", text: "100%" },
      preview: picture("Export complete 100%", 1),
      text: "Export complete 100%",
      headline: "100%",
      lines: ["Export complete 100%"],
      canWatchAgain: true,
    },
    setup: IDLE,
  };
  useWatch.getState().receive(samples[kind || "watching"] ?? watching);
}
