// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust side sends about the watched area (src-tauri/src/watch).

/** When the island speaks up (NotchWatchCondition). */
export type WatchCondition = "changes" | "settles" | "contains" | "reaches";
export const WATCH_CONDITIONS: readonly WatchCondition[] = ["changes", "settles", "contains", "reaches"];

/** Why a watch ended (NotchWatchOutcome). */
export type WatchOutcome =
  | { kind: "changed" }
  | { kind: "settled" }
  | { kind: "shows"; text: string }
  | { kind: "reached"; text: string }
  | { kind: "closed" };

export interface WatchTarget {
  /** Empty for a part of the display, which the page names Watch. */
  appName: string;
  windowTitle: string | null;
  /** A PNG data URL of the app's icon. */
  icon: string | null;
  isWindow: boolean;
}

export interface WatchSnapshot {
  phase: "idle" | "watching" | "hidden" | "finished";
  outcome: WatchOutcome | null;
  target: WatchTarget | null;
  /** A PNG data URL of the area as last read. */
  preview: string | null;
  text: string;
  headline: string;
  /** The line pinned to the island; null is automatic. */
  headlineLine: number | null;
  /** Up to 12 lines the person can pin, top to bottom. */
  lines: string[];
  /** Epoch ms. */
  startedAt: number | null;
  condition: WatchCondition;
  matchText: string;
  matchNumber: number | null;
  canWatchAgain: boolean;
  /** Windows refused to capture the area. */
  accessDenied: boolean;
}

/** Raised once when the rule is met or the window closes. */
export interface WatchFinished {
  outcome: WatchOutcome;
  appName: string;
  windowTitle: string | null;
}

export const IDLE: WatchSnapshot = {
  phase: "idle",
  outcome: null,
  target: null,
  preview: null,
  text: "",
  headline: "",
  headlineLine: null,
  lines: [],
  startedAt: null,
  condition: "changes",
  matchText: "",
  matchNumber: null,
  canWatchAgain: false,
  accessDenied: false,
};
