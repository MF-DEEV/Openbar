// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The settings window's copy of the preferences: loaded once, kept in step
// with every other window's changes, and written back one patch at a time.

import { create } from "zustand";
import { loadPrefs, onPrefs, type Prefs, type PrefsPatch, resolve, setPrefs } from "../shared/prefs";
import type { IslandSettings } from "../island/settings";
import type { SessionPrefs } from "../shared/prefs";

interface SettingsState {
  prefs: Prefs;
  loaded: boolean;
  error: string | null;
  patch(patch: PrefsPatch): void;
  island<K extends keyof IslandSettings>(key: K, value: IslandSettings[K]): void;
  sessions<K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]): void;
}

export const useSettings = create<SettingsState>((set, get) => ({
  prefs: resolve(null),
  loaded: false,
  error: null,
  patch(patch) {
    // Shown at once; the saved value comes back with the change notice.
    const before = get().prefs;
    set({
      prefs: {
        island: { ...before.island, ...(patch.island as Partial<IslandSettings>) },
        sessions: { ...before.sessions, ...(patch.sessions as Partial<SessionPrefs>) },
      },
      error: null,
    });
    setPrefs(patch)
      .then((prefs) => set({ prefs }))
      .catch((e: unknown) => set({ prefs: before, error: String(e) }));
  },
  island(key, value) {
    get().patch({ island: { [key]: value } });
  },
  sessions(key, value) {
    get().patch({ sessions: { [key]: value } });
  },
}));

export function startSettings() {
  void loadPrefs().then((prefs) => useSettings.setState({ prefs, loaded: true }));
  void onPrefs((prefs) => useSettings.setState({ prefs }));
}
