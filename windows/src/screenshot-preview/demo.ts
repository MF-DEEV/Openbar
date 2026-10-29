// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import type { Info } from "./Preview";

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720"><rect width="1280" height="720" fill="#171724"/><rect width="1280" height="42" fill="#292936"/><circle cx="24" cy="21" r="6" fill="#ff6159"/><circle cx="44" cy="21" r="6" fill="#ffbd2e"/><circle cx="64" cy="21" r="6" fill="#28c840"/><rect x="18" y="60" width="220" height="640" rx="10" fill="#222230"/><rect x="258" y="60" width="1000" height="640" rx="10" fill="#101018"/><text x="48" y="112" fill="#bbb" font-size="22" font-family="sans-serif">Vorssaint</text><text x="300" y="140" fill="#b8a4ff" font-size="30" font-family="monospace">const screenshot = await capture();</text><text x="300" y="196" fill="#8da0ad" font-size="24" font-family="monospace">// Preview your screenshot on the island</text><rect x="300" y="260" width="900" height="360" rx="18" fill="#28243e"/><circle cx="750" cy="420" r="95" fill="#7060a8"/><path d="M660 600L1040 320L1200 620H400Z" fill="#373052"/></svg>`;
export const demoInfo: Info = {
  image: `data:image/svg+xml,${encodeURIComponent(svg)}`,
  saved: "Screenshot.png",
  copied: true,
  duration: 0,
  dismissOnBlur: false,
  pinned: false,
};
