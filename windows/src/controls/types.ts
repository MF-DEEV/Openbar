// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The home page's items (NotchControlItem, NotchSupport.swift:1283-1411).

/** Every item the Mac has, in its declaration order (the default order). */
export const CONTROL_ITEMS = [
  "volume",
  "brightness",
  "keyboardLight",
  "music",
  "mixer",
  "keepAwake",
  "timer",
  "calendar",
  "microphone",
  "screenshot",
  "recording",
  "speedTest",
  "panel",
  "commandBar",
  "scratchpad",
] as const;

export type ControlItem = (typeof CONTROL_ITEMS)[number];

/** NotchControlItem.defaultHidden. */
export const DEFAULT_HIDDEN_CONTROLS: ControlItem[] = [
  "keyboardLight",
  "microphone",
  "screenshot",
  "recording",
  "speedTest",
  "panel",
  "commandBar",
  "scratchpad",
];

/** How an active tile reads (NotchTileAccent). */
export type TileAccent = "selection" | "alert" | "awake" | "timer";
