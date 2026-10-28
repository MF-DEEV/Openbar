// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The mixer's words, in the Mac's English (Localization, MixerFeatureStrings).

export const S = {
  section: "Volume mixer",
  empty: "Apps that use audio show up here",
  /** Windows' own: the audio service didn't answer. */
  unavailable: "Windows audio isn't responding",
  output: "Output",
  outputUnavailable: "Output unavailable",
  noOutputs: "No outputs found",
  couldNotSwitch: (error: string) => `Could not switch: ${error}`,
  chooseOutput: "Choose output",
  microphone: "Microphone",
  chooseMicrophone: "Choose microphone",
  noMicrophones: "No microphones found",
  micMute: "Mute microphone",
  micUnmute: "Unmute microphone",
  mute: "Mute",
  unmute: "Unmute",
  reset: "Reset to 100%",
  options: "Options",
  close: "Close",
  hideInactive: "Hide inactive apps",
  visibleApps: "Apps in the list",
  allShown: "All",
  hidden: (n: number) => `Hidden: ${n}`,
  hideFromList: "Hide from the list",
  pinFirst: "Pin to Front",
  unpin: "Unpin",
  moveLeft: "Move Left",
  moveRight: "Move Right",
  actions: "Actions",
  /** The Mac's Command-drag, on Windows' Ctrl. */
  arrange: "Hold Ctrl and drag to reorder",
  systemSounds: "System sounds",
} as const;
