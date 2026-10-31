// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, tf } from "../i18n/index";

// English copy for the screen selection (ScreenshotStrings.swift, enUS).

export const S = {
  get hintDrag() {
    return t("screenshot.hintDrag");
  },
  /** Watch's purpose, shown first (NotchWatchStrings.purpose). */
  get purposeWatch() {
    return t("notchWatch.purpose");
  },
  get hintFullScreen() {
    return t("windows.capture.hintFullScreen");
  },
  get hintPick() {
    return t("windows.capture.hintPick");
  },
  get hintNudge() {
    return t("windows.capture.hintNudge");
  },
  get hintCancel() {
    return t("screenshot.hintCancel");
  },
  size: (w: number, h: number) => tf("windows.capture.size", w, h),
} as const;
