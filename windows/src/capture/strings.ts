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
  hintTools: "1, 3 and 4 switch tools",
  hintRepeat: "R repeats the last area",
  controlsTitle: "Screen capture",
  toolScreenshot: "Screenshot",
  toolText: "Screen Text",
  toolColor: "Color Picker",
  repeatRegion: "R repeats the last area",
  collapse: "Collapse",
  close: "Close",
  size: (w: number, h: number) => `${w} × ${h}`,
} as const;
