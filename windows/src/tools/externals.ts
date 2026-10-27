// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Keep awake and Mic Mute belong to the Controls work (keep_awake.rs and
// audio/mic.rs). Tools reads them through this one adapter, as on/off
// switches with their state.

import type { UnlistenFn } from "@tauri-apps/api/event";
import { bridge } from "../shared/bridge";

export interface ToggleBackend {
  toggle(): Promise<unknown>;
  onState(handler: (on: boolean) => void): Promise<UnlistenFn>;
  current?(): Promise<boolean | undefined>;
}

export interface Externals {
  keepAwake?: ToggleBackend;
  mic?: ToggleBackend;
}

const keepAwake: ToggleBackend = {
  toggle: () => bridge.keepAwake.toggle(),
  onState: (handler) => bridge.keepAwake.onState((state) => handler(state.active)),
  current: async () => (await bridge.keepAwake.state())?.active,
};

const mic: ToggleBackend = {
  toggle: () => bridge.audio.micMute(),
  onState: (handler) => bridge.audio.onState((state) => handler(state.micMuted)),
  current: async () => (await bridge.audio.state())?.micMuted,
};

export const externals = (): Externals => ({ keepAwake, mic });
