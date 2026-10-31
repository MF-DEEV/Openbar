// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The home page's words, in the Mac's English (NotchStrings, Localization,
// BrightnessStrings, RecentCaptureStrings, RadialMenuStrings).

import { t } from "../i18n";

export const S = {
  get empty() {
    return t("notch.empty");
  },
  get volume() {
    return t("notch.volume");
  },
  get brightness() {
    return t("notch.brightness");
  },
  get settings() {
    return t("s.menuSettings");
  },
  get output() {
    return t("s.mediaOutput");
  },
  get outputUnavailable() {
    return t("s.mixerOutputUnavailable");
  },
  get mute() {
    return t("s.actionMute");
  },
  get unmute() {
    return t("s.actionUnmute");
  },
  get noDisplays() {
    return t("brightness.noDisplays");
  },
  get keepAwake() {
    return t("s.keepAwakeTitle");
  },
  get keepAwakeEndsIn() {
    return t("s.keepAwakeEndsIn");
  },
  get keepAwakeUntilOff() {
    return t("s.keepAwakeUntilDisabled");
  },
  get micMute() {
    return t("s.micMuteName");
  },
  get micUnmute() {
    return t("s.micUnmuteName");
  },
  get mixer() {
    return t("s.mixerSection");
  },
  get screenshot() {
    return t("screenshot.editorTitle");
  },
  get recording() {
    return t("recorder.pageTitle");
  },
  get speedTest() {
    return t("s.monitorItemNetTest");
  },
  get scratchpad() {
    return t("scratchpad.pageTitle");
  },
  get nothingPlaying() {
    return t("radialMenu.mediaNothingPlaying");
  },
  get nowPlaying() {
    return t("radialMenu.mediaNowPlaying");
  },
  get musicHint() {
    return t("notch.musicHint");
  },
  get playbackFailed() {
    return t("notchMusicExtras.playbackFailed");
  },
  get play() {
    return t("s.actionPlay");
  },
  get pause() {
    return t("s.actionPause");
  },
  get previous() {
    return t("wallpaper.previousPage");
  },
  get next() {
    return t("wallpaper.nextPage");
  },
  get muted() {
    return t("windows.controls.muted");
  },
} as const;
