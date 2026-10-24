// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every section page and live activity the Windows island has so far.

import { sessionsPage } from "../sessions/SessionsPage";
import { stripWing, readingWidth, stripTextSize, SessionsStrip } from "../sessions/SessionsStrip";
import { useSessions } from "../sessions/store";
import { geometryOf, useIsland } from "./store";
import { activityViews } from "./views/Activity";
import { pages } from "./views/Expanded";

pages.sessions = sessionsPage;
activityViews.sessions = SessionsStrip;

/** Sessions show beside the camera while any is working, waiting or done. */
function syncSessionsActivity() {
  const { counts } = useSessions.getState().snapshot;
  const island = useIsland.getState();
  const empty = counts.working + counts.waiting + counts.finished === 0;
  if (empty || island.settings.sections.sessions === false) {
    island.setActivity("sessions", null);
    return;
  }
  const geometry = geometryOf(island);
  const size = stripTextSize(geometry.stripHeight);
  island.setActivity("sessions", {
    wing: stripWing(counts, geometry.stripHeight),
    capsuleContent: 14 + 6 + readingWidth(counts, size),
    opens: "sessions",
  });
}

useSessions.subscribe((s, prev) => {
  if (s.snapshot.counts !== prev.snapshot.counts) syncSessionsActivity();
});
syncSessionsActivity();
