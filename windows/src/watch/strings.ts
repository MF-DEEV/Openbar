// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Watch section's English strings, as the Mac words them
// (NotchWatchStrings.swift, enUS), in Windows' sentence case and with the
// PC in place of the Mac.

import { t, tf } from "../i18n";

import type { WatchCondition, WatchOutcome } from "./types";

export const S = {
  get title() {
    return t("notchWatch.title");
  },
  get description() {
    return t("windows.watch.description");
  },
  get setupHint() {
    return t("notchWatch.setupHint");
  },
  get choose() {
    return t("windows.watch.choose");
  },
  get chooseAgain() {
    return t("notchWatch.chooseAgain");
  },
  get stop() {
    return t("notchWatch.stop");
  },
  get watchAgain() {
    return t("notchWatch.watchAgain");
  },
  get permissionHint() {
    return t("windows.watch.permissionHint");
  },
  get tellMe() {
    return t("notchWatch.tellMe");
  },
  get changes() {
    return t("notchWatch.changes");
  },
  get settles() {
    return t("notchWatch.settles");
  },
  get contains() {
    return t("notchWatch.contains");
  },
  get reaches() {
    return t("notchWatch.reaches");
  },
  get settlesHint() {
    return t("notchWatch.settlesHint");
  },
  get textPlaceholder() {
    return t("notchWatch.textPlaceholder");
  },
  get numberPlaceholder() {
    return t("notchWatch.numberPlaceholder");
  },
  get noText() {
    return t("notchWatch.noText");
  },
  get inIsland() {
    return t("notchWatch.inIsland");
  },
  get automatic() {
    return t("notchWatch.automatic");
  },
  get hidden() {
    return t("notchWatch.hidden");
  },
  get changedNotice() {
    return t("notchWatch.changedNotice");
  },
  get settledNotice() {
    return t("notchWatch.settledNotice");
  },
  shows: (text: string) => tf("notchWatch.showsFormat", text),
  reached: (text: string) => tf("notchWatch.reachedFormat", text),
  get closedNotice() {
    return t("notchWatch.closedNotice");
  },
  since: (time: string) => tf("notchWatch.sinceFormat", time),
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
