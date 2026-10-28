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
  // MixerOptionsControls, SoundOutputSwitcherControls, AudioPriorityDisclosure.
  lowerOnHeadphonesDisconnect: "Lower volume when headphones disconnect",
  lowerOnHeadphonesDisconnectCaption: "Adjusts output when wired or Bluetooth headphones disconnect.",
  headphonesDisconnectVolume: "Volume after disconnect",
  lower: "Lower",
  raise: "Raise",
  preciseRoller: "Use finer volume steps",
  preciseRollerCaption: "Turns volume wheels and keys into smaller system volume steps.",
  /** preciseVolumeRollerTapFailed. */
  preciseRollerFailed: "Could not listen for volume keys.",
  switcher: "Switch outputs with shortcut",
  switcherCaption: "Choose outputs and use the shortcut to move to the next available one.",
  switcherTitle: "Output switcher",
  switcherDevices: "Outputs in cycle",
  switcherNoAvailable: "Select at least one available output.",
  shortcut: "Shortcut",
  shortcutRecording: "Press the new shortcut",
  /** shortcutUnavailable, with Windows in place of macOS. */
  shortcutUnavailable: "Windows rejected this shortcut. Choose another one.",
  /** Windows' own: the island's toggle can't be given away. */
  shortcutTaken: "Ctrl+Alt+Space opens the island. Choose another one.",
  priority: "Audio priority",
  priorityCaption:
    "Devices are selected in priority order. When a higher-priority device connects, it becomes active. When the active one disconnects, the next available takes over.",
  priorityOutputs: "Output priority",
  priorityInputs: "Microphone priority",
  priorityOutputEnable: "Automatically switch to the highest-priority output",
  priorityInputEnable: "Automatically switch to the highest-priority microphone",
  priorityMoveUp: "Move up",
  priorityMoveDown: "Move down",
  priorityUnavailable: "Unavailable",
  priorityCurrent: "Current",
  /** mixerOutputCurrent, the "(current)" after a device's name. */
  current: "current",
} as const;
