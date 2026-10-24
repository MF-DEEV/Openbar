// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The wardrobe's English words, from NotchClaudeSessionStrings.swift. They
// move to the generated `notchClaudeSessions` table once the locale pipeline
// (spec 06-i18n) lands; until then the Wardrobe takes overrides by prop.

import type { ClawdEyewear, ClawdExtra, ClawdHat, ClawdShell } from "./costume";

export const clawdStrings = {
  wardrobe: "Wardrobe",
  dressUp: "Change outfit",
  dressUpFormat: "Change %@’s outfit",
  surpriseMe: "Surprise me",
  ownOutfit: "Back to the original outfit",
  goToSession: "Go to it",
  shellTitle: "Shell",
  hatTitle: "Hat",
  eyewearTitle: "Eyewear",
  extrasTitle: "Extras",
  nothing: "None",
  shells: {
    clay: "Clay",
    coral: "Coral",
    amber: "Amber",
    moss: "Moss",
    mint: "Mint",
    teal: "Teal",
    sky: "Sky",
    iris: "Iris",
    orchid: "Orchid",
    rose: "Rose",
  } as Record<ClawdShell, string>,
  hats: {
    beanie: "Beanie",
    cap: "Cap",
    beret: "Beret",
    topHat: "Top hat",
    crown: "Crown",
    party: "Party hat",
    propeller: "Propeller cap",
    chef: "Chef’s hat",
  } as Record<Exclude<ClawdHat, "none">, string>,
  eyewears: {
    glasses: "Glasses",
    shades: "Sunglasses",
    round: "Round glasses",
    monocle: "Monocle",
    anaglyph: "3D glasses",
  } as Record<Exclude<ClawdEyewear, "none">, string>,
  extras: {
    bow: "Bow",
    scarf: "Scarf",
    bowTie: "Bow tie",
    flower: "Flower",
    headphones: "Headphones",
    coffee: "Coffee",
  } as Record<Exclude<ClawdExtra, "none">, string>,
};

export type ClawdStrings = typeof clawdStrings;

/** "Change vorsaint’s outfit": the Clawd button's label names its session. */
export function dressUpLabel(name: string, strings: ClawdStrings = clawdStrings): string {
  return strings.dressUpFormat.replace("%@", name);
}
