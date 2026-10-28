// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The watched area as the island knows it, and what the page asks of the
// Rust side (NotchWatchService.swift). Reading, rules and timing live in
// src-tauri/src/watch; this keeps the last state, opens the area chooser
// with the island out of the way, and turns an ended watch into a notice.

import { Eye } from "lucide-react";
import { create } from "zustand";
import { modulesOf, useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { outcomeTitle, S } from "./strings";
import { IDLE, type WatchCondition, type WatchFinished, type WatchSnapshot } from "./types";

/** The island's collapse spring, with room for the window to repaint (tools/capture.ts). */
const SETTLE_MS = 350;

export const useWatch = create<{ snapshot: WatchSnapshot; receive(snapshot: WatchSnapshot): void }>()((set) => ({
  snapshot: IDLE,
  receive: (snapshot) => set({ snapshot }),
}));

/** The island is on and Watch is one of its sections. */
export function watchEnabled(): boolean {
  const island = useIsland.getState();
  return island.settings.enabled && modulesOf(island).includes("watch");
}

/** Watching or hidden: the closed island shows it. */
export const isActive = (s: WatchSnapshot) => s.target !== null && (s.phase === "watching" || s.phase === "hidden");

/** An area without text shows itself in the closed island instead. */
export const showsThumbnail = (s: WatchSnapshot) => s.headline === "" && s.preview !== null;

let choosing = false;

export const watch = {
  /** Collapses the island, lets the person drag an area, then opens on it. */
  async choose() {
    if (choosing || !watchEnabled()) return;
    choosing = true;
    try {
      const island = useIsland.getState();
      if (island.expanded) {
        island.collapse();
        await new Promise((resolve) => setTimeout(resolve, SETTLE_MS));
      }
      if (await bridge.watch.choose()) useIsland.getState().open("watch", { takeFocus: false });
    } finally {
      choosing = false;
    }
  },
  stop: () => void bridge.watch.stop(),
  again: () => void bridge.watch.again(),
  /** Saved for the next watch too, as the Mac's notchWatchCondition. */
  setCondition(condition: WatchCondition) {
    void setPrefs({ island: { watchCondition: condition } });
    void bridge.watch.setRule({ condition });
  },
  setText: (text: string) => void bridge.watch.setRule({ text }),
  setNumber: (number: string) => void bridge.watch.setRule({ number }),
  pinLine: (line: number | null) => void bridge.watch.pinLine(line),
};

/** The page is drawn: the island open on it, not on the sections gallery (system/register.tsx). */
export function pageVisible(): boolean {
  const s = useIsland.getState();
  return s.expanded && !s.showingSections && s.selected === "watch" && watchEnabled();
}

let sentVisible: boolean | null = null;
/** Tells Rust when the page shows, so the area is read every second while it does. */
export function syncPageVisible() {
  const visible = pageVisible();
  if (visible === sentVisible) return;
  sentVisible = visible;
  void bridge.watch.pageVisible(visible);
}

/**
 * The notice for an ended watch, unless its page is already open. Where the
 * island can't show it (hidden over a full-screen app) or another notice
 * holds the island, a Windows notification goes out instead.
 */
export function presentFinished(event: WatchFinished) {
  if (!watchEnabled()) return;
  const island = useIsland.getState();
  if (island.expanded && island.selected === "watch") return;
  const title = outcomeTitle(event.outcome);
  const id = `watch:${Date.now()}`;
  const hidden = island.attentionFullscreen && island.settings.hideInFullscreen;
  if (!hidden) {
    island.showNotice({
      id,
      kind: "watch",
      title,
      detail: event.appName || S.title,
      priority: 2,
      duration: 6,
      opens: "watch",
      icon: Eye,
    });
  }
  if (hidden || useIsland.getState().notice?.id !== id) {
    void bridge.watch.notify(title, event.windowTitle ?? (event.appName || S.title));
  }
}

export async function subscribeWatch() {
  const receive = (snapshot: WatchSnapshot) => useWatch.getState().receive(snapshot);
  await bridge.watch.onState(receive);
  await bridge.watch.onFinished(presentFinished);
  const first = await bridge.watch.snapshot();
  if (first) receive(first);
}
