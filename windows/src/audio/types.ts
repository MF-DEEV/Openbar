// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust audio module sends (event "audio://state"; audio/mod.rs),
// with keep awake (keep_awake.rs) and brightness (brightness.rs).

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

/** null while unknown: no device, or it refused to say. */
export interface AudioLevel {
  volume: number | null;
  muted: boolean | null;
}

export interface AudioApp {
  /** The app's exe path in lower case (its persistence id), or "system". */
  key: string;
  /** The exe whose icon the column shows. */
  path: string | null;
  name: string;
  volume: number;
  muted: boolean;
  playing: boolean;
  system: boolean;
}

export interface AudioState {
  supported: boolean;
  output: AudioLevel;
  input: AudioLevel;
  outputs: AudioDevice[];
  inputs: AudioDevice[];
  /** The default device can be switched (IPolicyConfig answered). */
  canSwitch: boolean;
  micMuted: boolean;
  apps: AudioApp[];
}

export interface KeepAwakeState {
  active: boolean;
  /** Epoch milliseconds; null runs until turned off. */
  endsAt: number | null;
}

export interface BrightnessDisplay {
  id: string;
  name: string;
  /** 0…1. */
  brightness: number;
  builtIn: boolean;
}

export const EMPTY_AUDIO: AudioState = {
  supported: false,
  output: { volume: null, muted: null },
  input: { volume: null, muted: null },
  outputs: [],
  inputs: [],
  canSwitch: false,
  micMuted: false,
  apps: [],
};
