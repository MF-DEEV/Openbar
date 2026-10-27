// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What calendar.rs reads from the Windows appointment store (spec 04 §3.5).
// Times are epoch milliseconds.

/** Whether the island may read the appointment store. */
export type CalendarAccess =
  /** No package identity, or one without the appointments capability. */
  | "unavailable"
  /** Windows has not asked the person yet. */
  | "notAsked"
  | "denied"
  | "allowed";

export interface CalendarEvent {
  /** One occurrence: the appointment's LocalId and its start. */
  id: string;
  title: string;
  /** The calendar's display name. */
  calendar: string;
  calendarId: string;
  start: number;
  end: number;
  allDay: boolean;
  location: string;
  /** `#rrggbb`. */
  color: string;
  /** What AppointmentManager opens; empty when unknown. */
  localId: string;
  /** The occurrence's original start in a series, for opening that one. */
  instanceStart: number | null;
  /** LocalId, or LocalId@originalStart in a series (NotchCalendarSupport.countdownKey). */
  countdownKey: string;
}

/** One calendar offered in Settings, grouped under its account. */
export interface CalendarChoice {
  id: string;
  title: string;
  sourceId: string;
  source: string;
  color: string;
}

export interface CalendarSnapshot {
  access: CalendarAccess;
  /** The range the page asked for, or the coming week. */
  events: CalendarEvent[];
  /** Always the coming week: what countdowns and the Controls tile follow. */
  current: CalendarEvent[];
  calendars: CalendarChoice[];
  /** The user's first day of the week, 0 = Sunday (LOCALE_IFIRSTDAYOFWEEK). */
  firstWeekday: number;
  /** Days of a new year the first week needs (LOCALE_IFIRSTWEEKOFYEAR: 1, 7 or 4). */
  minimumDaysInFirstWeek: number;
}
