// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A week of appointments for designing the Calendar section in a plain
// browser (?calendar): one under way, one in the countdown's hour and a few
// more over the coming days. ?calendar=denied or =unavailable shows the card.

import { addDays, startOfDay } from "./model";
import type { CalendarAccess, CalendarChoice, CalendarEvent, CalendarSnapshot } from "./types";

const calendars: CalendarChoice[] = [
  { id: "work", title: "Work", sourceId: "outlook", source: "Outlook", color: "#ff9f0a" },
  { id: "home", title: "Home", sourceId: "outlook", source: "Outlook", color: "#30d158" },
  { id: "birthdays", title: "Birthdays", sourceId: "local", source: "This PC", color: "#bf5af2" },
];

export function demoCalendar(access: CalendarAccess): CalendarSnapshot {
  const now = Date.now();
  const today = startOfDay(now);
  const minute = 60_000;
  const hour = 60 * minute;
  let n = 0;
  const event = (title: string, start: number, end: number, calendar = calendars[0]!, extra = {}): CalendarEvent => {
    const id = `demo-${n++}`;
    return {
      id: `${id}:${start}`,
      title,
      calendar: calendar.title,
      calendarId: calendar.id,
      start,
      end,
      allDay: false,
      location: "",
      color: calendar.color,
      localId: id,
      instanceStart: null,
      countdownKey: id,
      ...extra,
    };
  };
  const events = [
    event("Design review", now - 20 * minute, now + 25 * minute, calendars[0], { location: "Room 4.12" }),
    event("Standup", now + 34 * minute + 20_000, now + 49 * minute),
    event("Dentist", today + 18 * hour, today + 19 * hour, calendars[1], { location: "Kraków, ul. Długa 5" }),
    event("Anna's birthday", today, addDays(today, 1), calendars[2], { allDay: true }),
    event("Sprint planning", addDays(today, 1) + 10 * hour, addDays(today, 1) + 11 * hour + 30 * minute),
    event("Flight to Berlin", addDays(today, 2) + 21 * hour, addDays(today, 3) + 1 * hour, calendars[1]),
    event("Untitled", addDays(today, 4) + 9 * hour, addDays(today, 4) + 9 * hour + 30 * minute, calendars[0], {
      title: "",
    }),
    event("Retro", addDays(today, 9) + 15 * hour, addDays(today, 9) + 16 * hour),
  ];
  return {
    access,
    events,
    current: events,
    calendars,
    firstWeekday: 1,
    minimumDaysInFirstWeek: 4,
  };
}
