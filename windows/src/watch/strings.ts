// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Watch section's English strings, as the Mac words them
// (NotchWatchStrings.swift, enUS), in Windows' sentence case and with the
// PC in place of the Mac.

import type { WatchCondition, WatchOutcome } from "./types";

export const S = {
  title: "Watch",
  description:
    "Turn any part of any window into a live activity in the island, with an alert when it changes, finishes or shows what you are waiting for.",
  setupHint:
    "Choose any part of a window, like a progress bar, a build status or a score. The island reads it for you and tells you when it changes.",
  choose: "Choose area…",
  chooseAgain: "Choose another area",
  stop: "Stop watching",
  watchAgain: "Watch again",
  permissionHint:
    "Windows didn't let Vorssaint read this area. Watch reads it on this PC only; nothing leaves your PC.",
  tellMe: "Tell me",
  changes: "When it changes",
  settles: "When it stops changing",
  contains: "When it shows",
  reaches: "When the number reaches",
  settlesHint: "After it changes, it must stay still for 30 seconds.",
  textPlaceholder: "Text, like Done",
  numberPlaceholder: "Number, like 100",
  noText: "No text",
  inIsland: "Show in the island",
  automatic: "Automatic",
  hidden: "The window is hidden. Watching continues when it returns.",
  changedNotice: "It changed",
  settledNotice: "It stopped changing",
  shows: (text: string) => `It shows ${text}`,
  reached: (text: string) => `It reached ${text}`,
  closedNotice: "The window closed",
  since: (time: string) => `Watching since ${time}`,
} as const;

export function conditionTitle(condition: WatchCondition): string {
  return S[condition];
}

export function outcomeTitle(outcome: WatchOutcome): string {
  switch (outcome.kind) {
    case "changed":
      return S.changedNotice;
    case "settled":
      return S.settledNotice;
    case "shows":
      return S.shows(outcome.text);
    case "reached":
      return S.reached(outcome.text);
    case "closed":
      return S.closedNotice;
  }
}
