// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Watch section into the island: its page, its strip in the
// closed island (priority #2) while an area is watched or its window is
// hidden, and the notice when a watch ends.

import { geometryOf, useIsland } from "../island/store";
import { activityViews } from "../island/views/Activity";
import { pages } from "../island/views/Expanded";
import { isActive, subscribeWatch, useWatch, watchEnabled } from "./store";
import { WatchLeading, WatchPage, WatchTrailing } from "./WatchPage";
import { capsuleContent, stripWing, WatchStrip } from "./WatchStrip";
import "./watch.css";

pages.watch = { Page: WatchPage, Leading: WatchLeading, Trailing: WatchTrailing };
activityViews.watch = WatchStrip;

function syncActivity() {
  const island = useIsland.getState();
  const { snapshot } = useWatch.getState();
  if (!watchEnabled() || !isActive(snapshot)) {
    if (island.activities.watch) island.setActivity("watch", null);
    return;
  }
  const wing = stripWing(snapshot, geometryOf(island).stripHeight);
  const content = capsuleContent(snapshot);
  const current = island.activities.watch;
  if (current?.wing === wing && current.capsuleContent === content) return;
  island.setActivity("watch", { wing, capsuleContent: content, opens: "watch" });
}

useWatch.subscribe(syncActivity);
useIsland.subscribe((s, prev) => {
  if (s.settings !== prev.settings || s.screen !== prev.screen) syncActivity();
});
void subscribeWatch();
