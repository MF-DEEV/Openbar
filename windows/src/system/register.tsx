// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the System section into the island: its page, and the monitor that
// samples only while the page is on screen (SystemMonitor's consumers). The
// System section has no live activity (spec 04 §8).

import { modulesOf, useIsland } from "../island/store";
import { pages } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import { useSystem } from "./store";
import { SystemPage } from "./SystemPage";

pages.system = { Page: SystemPage };

/** The page is drawn: the island open on it, not on the sections gallery. */
function pageVisible(): boolean {
  const s = useIsland.getState();
  return (
    s.settings.enabled && s.expanded && !s.showingSections && s.selected === "system" && modulesOf(s).includes("system")
  );
}

let sent = "";
/** Starts or stops sampling, and leaving the page closes its detail (NotchService.collapse, select). */
function syncWatch() {
  const visible = pageVisible();
  if (!visible && useSystem.getState().detail) useSystem.getState().closeDetail();
  const detail = visible ? useSystem.getState().detail : null;
  const key = `${visible}|${detail}`;
  if (key === sent) return;
  sent = key;
  void bridge.system.watch(visible, detail);
  // Opened before the monitor's first probe landed: take what it knows now.
  if (visible && !useSystem.getState().snapshot?.seq) void fetchStored();
}

async function fetchStored() {
  const stored = await bridge.system.snapshot();
  if (stored && !useSystem.getState().snapshot?.seq) useSystem.getState().receive(stored);
}

useIsland.subscribe(syncWatch);
useSystem.subscribe((s, prev) => {
  if (s.detail !== prev.detail) syncWatch();
});

/** Listen first, so the stored reading fetched next can't overwrite a newer one. */
export async function subscribeSystem() {
  await bridge.system.onSnapshot((snapshot) => useSystem.getState().receive(snapshot));
  await bridge.system.onSpeedTest((state) => useSystem.getState().receiveSpeedTest(state));
  await fetchStored();
  const speedTest = await bridge.system.speedTest();
  if (speedTest) useSystem.getState().receiveSpeedTest(speedTest);
}
void subscribeSystem();
