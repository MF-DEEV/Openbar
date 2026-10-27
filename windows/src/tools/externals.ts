// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Keep awake and microphone mute live in the Controls work, built on another
// branch. Their tools and the mic row read the bridge through this file, so
// once `bridge.keepAwake` and `bridge.mic` exist they light up with no other
// change; until then both are missing and their tools stay hidden.
//
// Assumed names (wire these in src/shared/bridge.ts):
//   bridge.keepAwake.toggle(): Promise<unknown>
//   bridge.keepAwake.onState(handler: (on: boolean) => void): Promise<UnlistenFn>
//   bridge.mic.toggle(): Promise<unknown>
//   bridge.mic.onState(handler: (muted: boolean) => void): Promise<UnlistenFn>
// `onState` should report the current state soon after subscribing; an
// optional `current(): Promise<boolean | undefined>` is read once if present.

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

/** Read on each call, so a bridge wired later is picked up. */
export const externals = (): Externals => bridge as unknown as Externals;
