// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The settings window's copy of the preferences: loaded once, kept in step
// with every other window's changes, and written back one patch at a time.
//
// What shows is the last saved state with every patch still on its way laid
// over it. A slider sends a patch per step, and each answer carries the state
// after *its* step, so taking an answer as it is would pull the thumb back
// under the pointer; a failed patch drops out and the rest stay.

import { create } from "zustand";
import {
  loadPrefs,
  mergePatch,
  onPrefs,
  type Prefs,
  type PrefsPatch,
  resolve,
  type SessionPrefs,
  setPrefs,
} from "../shared/prefs";
import type { IslandSettings } from "../island/settings";

interface SettingsState {
  prefs: Prefs;
  loaded: boolean;
  error: string | null;
  patch(patch: PrefsPatch): void;
  /** Global shortcuts display only what Windows accepted and saved. */
  shortcut(change: { shortcut?: string; shortcutEnabled?: boolean }): Promise<void>;
  island<K extends keyof IslandSettings>(key: K, value: IslandSettings[K]): void;
  sessions<K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]): void;
}

let saved: Prefs = resolve(null);
let pending: { id: number; patch: PrefsPatch }[] = [];
let nextId = 0;

/** The saved state with the pending patches on top. */
function shown(): Prefs {
  if (pending.length === 0) return saved;
  const layered = structuredClone(saved) as unknown as Record<string, unknown>;
  for (const p of pending) mergePatch(layered, structuredClone(p.patch) as Record<string, unknown>);
  return resolve(layered);
}

export const useSettings = create<SettingsState>((set, get) => ({
  prefs: saved,
  loaded: false,
  error: null,
  async shortcut(change) {
    const prefs = await setPrefs({ island: change });
    received(prefs);
  },
  patch(patch) {
    const id = nextId++;
    pending.push({ id, patch });
    set({ prefs: shown(), error: null });
    setPrefs(patch)
      .then((prefs) => {
        saved = prefs;
        pending = pending.filter((p) => p.id !== id);
        set({ prefs: shown() });
      })
      .catch((e: unknown) => {
        pending = pending.filter((p) => p.id !== id);
        set({ prefs: shown(), error: String(e) });
      });
  },
  island(key, value) {
    get().patch({ island: { [key]: value } });
  },
  sessions(key, value) {
    get().patch({ sessions: { [key]: value } });
  },
}));

/** A change from any window (this one included) is the new saved state. */
function received(prefs: Prefs) {
  saved = prefs;
  useSettings.setState({ prefs: shown() });
}

export function startSettings() {
  void loadPrefs().then((prefs) => {
    received(prefs);
    useSettings.setState({ loaded: true });
  });
  void onPrefs(received);
}

/** For tests: back to a fresh window. */
export function resetSettings(prefs: Prefs = resolve(null)) {
  saved = prefs;
  pending = [];
  useSettings.setState({ prefs, error: null, loaded: false });
}
