// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The displays the brightness level can set (BrightnessService). Read when
// the home page shows, never polled: DDC/CI reads are slow.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import type { BrightnessDisplay } from "../audio/types";

interface BrightnessStore {
  displays: BrightnessDisplay[];
  /** The display picked in the chooser this session. */
  picked: string | null;
  loaded: boolean;
  refresh(): Promise<void>;
  pick(id: string): void;
  set(id: string, value: number): void;
}

/**
 * The display the slider moves: the one picked, else the built-in panel,
 * else the first. Windows reports no "display the keys last moved"
 * (spec 05 §1.3), so the built-in panel stands in for it.
 */
export function chosenDisplay(displays: readonly BrightnessDisplay[], picked: string | null) {
  return displays.find((d) => d.id === picked) ?? displays.find((d) => d.builtIn) ?? displays[0] ?? null;
}

export const useBrightness = create<BrightnessStore>()((set, get) => ({
  displays: [],
  picked: null,
  loaded: false,

  async refresh() {
    const displays = await bridge.brightness.displays();
    set({ displays: displays ?? [], loaded: true });
  },

  pick: (id) => set({ picked: id }),

  set(id, value) {
    const level = Math.min(1, Math.max(0, value));
    set({ displays: get().displays.map((d) => (d.id === id ? { ...d, brightness: level } : d)) });
    // The backend keeps only the newest level and writes ten times a second at most.
    void bridge.brightness.set(id, level).catch(() => {});
  },
}));
