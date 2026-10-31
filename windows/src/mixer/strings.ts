// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The mixer's words, in the Mac's English (Localization, MixerFeatureStrings).

import { t, tf } from "../i18n";

export const S = {
  get section() {
    return t("s.mixerSection");
  },
  get empty() {
    return t("s.mixerEmpty");
  },
  /** Windows' own: the audio service didn't answer. */
  get unavailable() {
    return t("windows.mixer.unavailable");
  },
  get output() {
    return t("s.mixerSystemOutputTitle");
  },
  get outputUnavailable() {
    return t("s.mixerOutputUnavailable");
  },
  get noOutputs() {
    return t("s.mixerSystemOutputNoDevices");
  },
  couldNotSwitch: (error: string) => tf("s.mixerInputErrorFormat", error),
  get chooseOutput() {
    return t("s.mixerOutputTooltip");
  },
  get microphone() {
    return t("s.mixerInputTitle");
  },
  get chooseMicrophone() {
    return t("s.mixerInputTooltip");
  },
  get noMicrophones() {
    return t("s.mixerInputNoDevices");
  },
  get micMute() {
    return t("s.micMuteName");
  },
  get micUnmute() {
    return t("s.micUnmuteName");
  },
  get mute() {
    return t("s.actionMute");
  },
  get unmute() {
    return t("s.actionUnmute");
  },
  get reset() {
    return t("s.mixerResetTooltip");
  },
  get options() {
    return t("s.keepAwakeOptions");
  },
  get close() {
    return t("permissionGuide.closeHelp");
  },
  get hideInactive() {
    return t("mixer.hideInactiveApps");
  },
  get visibleApps() {
    return t("s.mixerVisibleApps");
  },
  get allShown() {
    return t("s.mixerAllShown");
  },
  hidden: (n: number) => tf("windows.mixer.hidden", n),
  get hideFromList() {
    return t("s.mixerHideFromList");
  },
  get pinFirst() {
    return t("mixer.pinFirst");
  },
  get unpin() {
    return t("mixer.unpin");
  },
  get moveLeft() {
    return t("mixer.moveLeft");
  },
  get moveRight() {
    return t("mixer.moveRight");
  },
  get actions() {
    return t("mixer.actions");
  },
  /** The Mac's Command-drag, on Windows' Ctrl. */
  get arrange() {
    return t("windows.mixer.arrange");
  },
  get systemSounds() {
    return t("s.mixerSoundEffectsOutputTitle");
  },
  // MixerOptionsControls, SoundOutputSwitcherControls, AudioPriorityDisclosure.
  get lowerOnHeadphonesDisconnect() {
    return t("s.mixerLowerOnHeadphonesDisconnect");
  },
  get lowerOnHeadphonesDisconnectCaption() {
    return t("s.mixerLowerOnHeadphonesDisconnectCaption");
  },
  get headphonesDisconnectVolume() {
    return t("s.mixerHeadphonesDisconnectVolume");
  },
  get lower() {
    return t("windows.mixer.lower");
  },
  get raise() {
    return t("windows.mixer.raise");
  },
  get preciseRoller() {
    return t("s.preciseVolumeRollerEnable");
  },
  get preciseRollerCaption() {
    return t("s.preciseVolumeRollerCaption");
  },
  /** preciseVolumeRollerTapFailed. */
  get preciseRollerFailed() {
    return t("s.preciseVolumeRollerTapFailed");
  },
  get switcher() {
    return t("s.soundOutputSwitcherEnable");
  },
  get switcherCaption() {
    return t("s.soundOutputSwitcherCaption");
  },
  get switcherTitle() {
    return t("s.soundOutputSwitcherTitle");
  },
  get switcherDevices() {
    return t("s.soundOutputSwitcherDevices");
  },
  get switcherNoAvailable() {
    return t("s.soundOutputSwitcherNoAvailableSelection");
  },
  /** shelfHotkeyLabel. */
  get shortcut() {
    return t("commandBar.appShortcutLabel");
  },
  /** shortcutPressKeys, on the field while it records. */
  get shortcutPressKeys() {
    return t("s.shortcutPressKeys");
  },
  /** ShortcutRecordingCaption: shortcutRecording and shortcutEscapeHint. */
  get shortcutRecordingCaption() {
    return t("windows.mixer.shortcutRecordingCaption");
  },
  get shortcutReset() {
    return t("notchAgents.confirmReset");
  },
  /** shortcutInvalid, with Windows' modifier names. */
  get shortcutInvalid() {
    return t("windows.mixer.shortcutInvalid");
  },
  /** shortcutUnavailable, with Windows in place of macOS. */
  get shortcutUnavailable() {
    return t("windows.mixer.shortcutUnavailable");
  },
  /** shortcutConflictFormat, naming what already holds the keys. */
  shortcutConflict: (who: "island" | "sessions") =>
    tf(
      "s.shortcutConflictFormat",
      who === "island" ? t("windows.mixer.shortcutConflict") : t("windows.mixer.shortcutConflicta2974a"),
    ),
  get priority() {
    return t("s.audioPrioritySection");
  },
  get priorityCaption() {
    return t("s.audioPriorityCaption");
  },
  get priorityOutputs() {
    return t("s.audioPriorityOutputList");
  },
  get priorityInputs() {
    return t("s.audioPriorityInputList");
  },
  get priorityOutputEnable() {
    return t("s.audioPriorityOutputEnable");
  },
  get priorityInputEnable() {
    return t("s.audioPriorityInputEnable");
  },
  get priorityMoveUp() {
    return t("mixer.moveUp");
  },
  get priorityMoveDown() {
    return t("mixer.moveDown");
  },
  get priorityUnavailable() {
    return t("notchShape.unavailable");
  },
  get priorityCurrent() {
    return t("s.mixerOutputCurrent");
  },
  /** mixerOutputCurrent, the "(current)" after a device's name. */
  get current() {
    return t("s.mixerOutputCurrent");
  },
} as const;
