// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// English copy for the screen selection (ScreenshotStrings.swift, enUS).

import { t, tf } from "../i18n";

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
  get hintTools() {
    return t("windows.capture.hintTools");
  },
  get hintRepeat() {
    return t("screenshot.hintRepeat");
  },
  get controlsTitle() {
    return t("screenshot.screenCaptureTitle");
  },
  get toolScreenshot() {
    return t("screenshot.editorTitle");
  },
  get toolText() {
    return t("windows.capture.toolText");
  },
  get toolColor() {
    return t("s.colorPickerName");
  },
  get repeatRegion() {
    return t("screenshot.hintRepeat");
  },
  get collapse() {
    return t("notch.collapse");
  },
  get close() {
    return t("permissionGuide.closeHelp");
  },
  get toolRecording() {
    return t("recorder.pageTitle");
  },
  get systemAudio() {
    return t("windows.capture.systemAudio");
  },
  get microphone() {
    return t("recorder.microphoneTrackLabel");
  },
  size: (w: number, h: number) => tf("windows.capture.size", w, h),
} as const;
