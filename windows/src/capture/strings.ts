// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// English copy for the screen selection (ScreenshotStrings.swift, enUS).

export const S = {
  hintDrag: "Drag to select an area",
  /** Watch's purpose, shown first (NotchWatchStrings.purpose). */
  purposeWatch: "Choose what to watch",
  hintFullScreen: "Enter captures the whole screen",
  hintPick: "Click to copy the color",
  hintNudge: "Arrow keys move one pixel",
  hintCancel: "Esc cancels",
  size: (w: number, h: number) => `${w} × ${h}`,
} as const;
