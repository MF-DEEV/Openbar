// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Timer section into the island: its page, its strip in the
// closed island (priority #1), the page's height, and the clock's catch-up
// after sleep, hiding or Rust's wake-up at the deadline.

import { geometryOf, useIsland } from "../island/store";
import { activityViews } from "../island/views/Activity";
import { pages } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import * as model from "./model";
import { onTimerDrafts } from "./prefs";
import { capsuleContent, stripWing, TimerStrip } from "./TimerStrip";
import { syncPageHeight, timerPage } from "./TimerPage";
import { catchUp, timer, timerEnabled, useTimer } from "./store";

pages.timer = timerPage;
activityViews.timer = TimerStrip;

/** The strip shows while a session exists, unless its countdown is hidden; a finished one always shows. */
function syncActivity() {
  const island = useIsland.getState();
  const { session, now } = useTimer.getState();
  if (!model.showsActivity(session, timerEnabled(), island.settings.hideTimerCountdown)) {
    if (island.activities.timer) island.setActivity("timer", null);
    return;
  }
  const reading = model.compactText(session, now);
  const wing = stripWing(reading, geometryOf(island).stripHeight);
  const content = capsuleContent(reading);
  const current = island.activities.timer;
  if (current?.wing === wing && current.capsuleContent === content) return;
  island.setActivity("timer", { wing, capsuleContent: content, opens: "timer" });
}

useTimer.subscribe(() => {
  syncActivity();
  syncPageHeight();
});
useIsland.subscribe((s, prev) => {
  if (s.settings !== prev.settings) timer.syncWithPreferences();
  if (s.settings !== prev.settings || s.screen !== prev.screen) {
    syncActivity();
    syncPageHeight();
  }
});
onTimerDrafts(syncPageHeight);
syncPageHeight();

// Re-measure the clock whenever the webview may have slept or been throttled.
catchUp();
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") catchUp();
});
void bridge.timer.onDue(catchUp);
