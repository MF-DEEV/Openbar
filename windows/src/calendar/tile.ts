// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Controls page's Calendar tile (NotchCalendarTile in
// NotchControlsView.swift; spec 04 §3.6): the next timed start this week,
// its title and when it starts, or the section's name with no reading.

import { displayTitle, tileEvent, tileStartText } from "./model";
import { useCalendar } from "./store";
import { C } from "./strings";
import { useMinute } from "./useMinute";

export interface CalendarTile {
  title: string;
  /** `HH:mm` today, else `EEE HH:mm`; null without an event. */
  reading: string | null;
}

export function useCalendarTile(): CalendarTile {
  const current = useCalendar((s) => s.current);
  const allowed = useCalendar((s) => s.access === "allowed");
  const now = useMinute();
  const event = allowed ? tileEvent(current, now) : null;
  return event
    ? { title: displayTitle(event, C.untitled), reading: tileStartText(event.start, now) }
    : { title: C.title, reading: null };
}
