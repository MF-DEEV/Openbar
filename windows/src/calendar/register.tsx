// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Calendar section into the island: its page with the month title
// and arrows in the top row, and its countdown in the closed island, which
// takes it in its lead window (priority #6, spec 04 §0.1, §3.6).

import { geometryOf, useIsland } from "../island/store";
import { activityViews } from "../island/views/Activity";
import { pages } from "../island/views/Expanded";
import { CalendarPage, MonthTitle, Navigation } from "./CalendarPage";
import { CalendarStrip, stripSpec } from "./CalendarStrip";
import { type Countdown, countdownTransition } from "./model";
import { registerTileReading } from "../controls/tiles";
import { useCalendarTile } from "./tile";
import { calendarShown, currentCountdown, followOf, subscribeCalendar, useCalendar } from "./store";

pages.calendar = { Page: CalendarPage, Leading: MonthTitle, Trailing: Navigation };
activityViews.calendar = CalendarStrip;

/** A read gives new objects for the same event; only a different event or moment redraws. */
const same = (a: Countdown | null, b: Countdown | null) => JSON.stringify(a) === JSON.stringify(b);

let transition: ReturnType<typeof setTimeout> | undefined;

/**
 * Who the closed island counts down to, now and whenever a followed hour
 * next opens or closes; the strip itself ticks every second while shown.
 */
export function syncCountdown() {
  clearTimeout(transition);
  const now = Date.now();
  const next = currentCountdown(now);
  const state = useCalendar.getState();
  if (!same(next, state.countdown)) {
    useCalendar.setState({ countdown: next, lastCountdown: next ?? state.countdown ?? state.lastCountdown });
  }
  const island = useIsland.getState();
  const spec = next ? stripSpec(next, geometryOf(island)) : null;
  const before = island.activities.calendar;
  if (spec?.wing !== before?.wing || spec?.capsuleContent !== before?.capsuleContent) {
    island.setActivity("calendar", spec);
  }
  if (!calendarShown() || state.access !== "allowed") return;
  const at = countdownTransition(state.current, now, followOf(island.settings));
  // Timers hold 2^31 ms at most; a later moment is looked at again on the next read.
  if (at !== null && at - now < 2 ** 31 - 1) transition = setTimeout(syncCountdown, at - now + 50);
}

useCalendar.subscribe((s, prev) => {
  if (s.current !== prev.current || s.access !== prev.access) syncCountdown();
});
useIsland.subscribe((s, prev) => {
  if (s.settings !== prev.settings || s.screen !== prev.screen) syncCountdown();
});
void subscribeCalendar();

// The Controls tile names the next timed event this week and when it starts.
registerTileReading("calendar", () => {
  const tile = useCalendarTile();
  return tile.reading ? { title: tile.title, reading: tile.reading } : null;
});
