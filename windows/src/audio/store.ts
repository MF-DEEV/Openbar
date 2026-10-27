// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's view of system audio, shared by Controls and the Mixer. A
// level the person sets shows at once and holds for a moment against older
// readings still on their way, so a dragged slider never jumps back.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { EMPTY_AUDIO, type AudioApp, type AudioLevel, type AudioState } from "./types";

/** How long a set level wins over readings from the system. */
export const HOLD_MS = 800;

/** A held field: "output.volume", "input.muted", "app:<key>.volume"… */
type Holds = Record<string, { value: number | boolean; until: number }>;

interface AudioStore {
  state: AudioState;
  /** The audio thread has answered once; before that, "unsupported" means nothing yet. */
  received: boolean;
  holds: Holds;
  /** The last switch or setting that failed, for the band's label. */
  error: string | null;
  receive(state: AudioState): void;
  setOutput(change: { volume?: number; muted?: boolean }): void;
  setInput(change: { volume?: number; muted?: boolean }): void;
  setApp(key: string, change: { volume?: number; muted?: boolean }): void;
  setDefault(id: string): Promise<void>;
  toggleMicMute(): Promise<void>;
}

function held<T extends number | boolean | null>(holds: Holds, field: string, value: T, now: number): T {
  const hold = holds[field];
  return hold && hold.until > now ? (hold.value as T) : value;
}

function heldLevel(holds: Holds, prefix: string, level: AudioLevel, now: number): AudioLevel {
  return {
    volume: held(holds, `${prefix}.volume`, level.volume, now),
    muted: held(holds, `${prefix}.muted`, level.muted, now),
  };
}

/** A reading with every field the person set in the last moment kept as set. */
export function applyHolds(state: AudioState, holds: Holds, now = Date.now()): AudioState {
  if (Object.keys(holds).length === 0) return state;
  return {
    ...state,
    output: heldLevel(holds, "output", state.output, now),
    input: heldLevel(holds, "input", state.input, now),
    micMuted: held(holds, "mic.muted", state.micMuted, now),
    apps: state.apps.map((app): AudioApp => ({
      ...app,
      volume: held(holds, `app:${app.key}.volume`, app.volume, now),
      muted: held(holds, `app:${app.key}.muted`, app.muted, now),
    })),
  };
}

/** Drops holds that ran out. */
function pruned(holds: Holds, now: number): Holds {
  return Object.fromEntries(Object.entries(holds).filter(([, h]) => h.until > now));
}

function hold(holds: Holds, prefix: string, change: { volume?: number; muted?: boolean }): Holds {
  const now = Date.now();
  const next = pruned(holds, now);
  if (change.volume !== undefined) {
    next[`${prefix}.volume`] = { value: change.volume, until: now + HOLD_MS };
    // A level above zero unmutes, as the system does (audio/engine.rs).
    if (change.volume > 0 && change.muted === undefined)
      next[`${prefix}.muted`] = { value: false, until: now + HOLD_MS };
  }
  if (change.muted !== undefined) next[`${prefix}.muted`] = { value: change.muted, until: now + HOLD_MS };
  return next;
}

const clamp = (v: number) => Math.min(1, Math.max(0, v));
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const useAudio = create<AudioStore>()((set, get) => {
  const report = (promise: Promise<unknown>) =>
    promise.then(
      () => set({ error: null }),
      (e) => set({ error: message(e) }),
    );
  return {
    state: EMPTY_AUDIO,
    received: false,
    holds: {},
    error: null,

    receive(state) {
      set({ state: applyHolds(state, get().holds), received: true });
    },

    setOutput(change) {
      const volume = change.volume === undefined ? undefined : clamp(change.volume);
      const holds = hold(get().holds, "output", { ...change, volume });
      set({ holds, state: applyHolds(get().state, holds) });
      void report(bridge.audio.setOutput(volume, change.muted));
    },

    setInput(change) {
      const volume = change.volume === undefined ? undefined : clamp(change.volume);
      const holds = hold(get().holds, "input", { ...change, volume });
      set({ holds, state: applyHolds(get().state, holds) });
      void report(bridge.audio.setInput(volume, change.muted));
    },

    setApp(key, change) {
      const volume = change.volume === undefined ? undefined : clamp(change.volume);
      const holds = hold(get().holds, `app:${key}`, { ...change, volume });
      set({ holds, state: applyHolds(get().state, holds) });
      void report(bridge.audio.setApp(key, volume, change.muted));
    },

    async setDefault(id) {
      try {
        await bridge.audio.setDefault(id);
        set({ error: null });
      } catch (e) {
        set({ error: message(e) });
      }
    },

    async toggleMicMute() {
      const target = !get().state.micMuted;
      const holds = { ...pruned(get().holds, Date.now()), "mic.muted": { value: target, until: Date.now() + HOLD_MS } };
      set({ holds, state: applyHolds(get().state, holds) });
      try {
        await bridge.audio.micMute(target);
        set({ error: null });
      } catch (e) {
        set({ error: message(e) });
      }
    },
  };
});

/** Listens first, so the first reading can't overwrite a newer event. */
export async function subscribeAudio(): Promise<void> {
  let version = 0;
  await bridge.audio.onState((state) => {
    version++;
    useAudio.getState().receive(state);
  });
  const at = version;
  const first = await bridge.audio.state();
  if (first && version === at) useAudio.getState().receive(first);
}

/** The level a slider shows: none when muted (NotchAudioControls.level). */
export function shownLevel(level: AudioLevel): number | null {
  if (level.volume === null) return null;
  return level.muted ? 0 : level.volume;
}

/** "n%" of a 0…1 level; a level that isn't a number reads 0% (MixerRoutingSupport.displayPercent). */
export function percent(volume: number): number {
  return Number.isFinite(volume) ? Math.round(Math.min(1000, Math.max(-1000, volume)) * 100) : 0;
}
