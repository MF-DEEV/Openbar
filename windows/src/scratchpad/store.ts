// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The pads in memory and on disk (ScratchpadService): loaded when the page
// first shows, saved 0.8 s after the last edit and right away when the page
// goes, never over a file that couldn't be read.

import { create } from "zustand";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { applyRetention, initialDocument } from "./model";
import { S } from "./strings";
import type { ScratchpadDocument } from "./types";

export interface ScratchpadState {
  doc: ScratchpadDocument | null;
  loadFailed: boolean;
  saveFailed: boolean;
  previewing: boolean;
  /** The pad being renamed, or closed after a confirmation. */
  renaming: string | null;
  closing: string | null;
}

export const useScratchpad = create<ScratchpadState>(() => ({
  doc: null,
  loadFailed: false,
  saveFailed: false,
  previewing: false,
  renaming: null,
  closing: null,
}));

const SAVE_DELAY_MS = 800;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let loading: Promise<void> | null = null;

/** Reads the pads once; every opening then clears text past its retention. */
export async function openScratchpad() {
  loading ??= (async () => {
    try {
      const stored = await bridge.scratchpad.load();
      const doc = stored ?? useScratchpad.getState().doc ?? initialDocument(S.pageTitle);
      useScratchpad.setState({ doc, loadFailed: false });
    } catch {
      loading = null;
      useScratchpad.setState({ loadFailed: true });
    }
  })();
  await loading;
  const { doc } = useScratchpad.getState();
  if (!doc) return;
  const kept = applyRetention(doc, useIsland.getState().settings.scratchpadRetention, Date.now());
  if (kept !== doc) update(kept);
}

export function update(doc: ScratchpadDocument) {
  useScratchpad.setState({ doc });
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void flush(), SAVE_DELAY_MS);
}

export async function flush() {
  clearTimeout(saveTimer);
  saveTimer = undefined;
  const { doc, loadFailed } = useScratchpad.getState();
  if (!doc || loadFailed) return;
  try {
    await bridge.scratchpad.save(doc);
    useScratchpad.setState({ saveFailed: false });
  } catch {
    useScratchpad.setState({ saveFailed: true });
  }
}
