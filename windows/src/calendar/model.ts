// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { dateTimeFormat } from "../i18n/index";

// The calendar's rules, free of React and Tauri (NotchCalendarSupport.swift;
// spec 04 §3.1-3.2). Times are epoch milliseconds; days are local midnights.

import type { CalendarChoice, CalendarEvent } from "./types";

const MINUTE = 60_000;
/** Each countdown shows during the hour before its moment (countdownLeadTime). */
export const LEAD_TIME = 60 * MINUTE;
/** A calendar without a colour of its own (NotchCalendarColor.fallback). */
export const FALLBACK_COLOR = "#59a6ff";

// MARK: Days

export function startOfDay(t: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** The same wall-clock time `n` days on, across daylight-saving changes. */
export function addDays(t: number, n: number): number {
  const d = new Date(t);
  d.setDate(d.getDate() + n);
  return d.getTime();
}

export function startOfMonth(t: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
}

export function addMonths(t: number, n: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
}

export const sameDay = (a: number, b: number) => startOfDay(a) === startOfDay(b);
export const sameMonth = (a: number, b: number) => startOfMonth(a) === startOfMonth(b);

/** Days from the week's first day (0 = Sunday) back to `t`'s day. */
const weekOffset = (t: number, firstWeekday: number) => (new Date(t).getDay() - firstWeekday + 7) % 7;

/** The 42 days of the month grid around `month`, from the first weekday. */
export function monthDays(month: number, firstWeekday: number): number[] {
  const first = startOfMonth(month);
  const start = addDays(first, -weekOffset(first, firstWeekday));
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

/** The seven days around `date`. Every week lies inside the grid `monthDays` gives for any of its days. */
export function weekDays(date: number, firstWeekday: number): number[] {
  const day = startOfDay(date);
  const start = addDays(day, -weekOffset(day, firstWeekday));
  return Array.from({ length: 7 }, (_, i) => addDays(start, i));
}

export const startsWeek = (day: number, firstWeekday: number) => new Date(day).getDay() === firstWeekday;

/**
 * The number beside a row of the month grid, by the locale's own rules: its
 * first weekday and how many days of January the first week needs (1 in the
 * US, 4 for ISO). Every day of a row shares it (NotchCalendarSupport.weekNumber).
 */
export function weekNumber(day: number, firstWeekday: number, minimumDaysInFirstWeek: number): number {
  const weekStart = startOfDay(addDays(day, -weekOffset(day, firstWeekday)));
  const firstWeek = (year: number) => {
    const january = new Date(year, 0, 1).getTime();
    const start = addDays(january, -weekOffset(january, firstWeekday));
    return 7 - weekOffset(january, firstWeekday) >= minimumDaysInFirstWeek ? start : addDays(start, 7);
  };
  const year = new Date(day).getFullYear();
  let first = firstWeek(year + 1);
  if (weekStart < first) first = firstWeek(year);
  if (weekStart < first) first = firstWeek(year - 1);
  return Math.round((weekStart - first) / (7 * 24 * 60 * MINUTE)) + 1;
}

/** Rows of the month grid a short island opens: 14 pt of weekdays, six rows of 16-30 (NotchLayout). */
export const MONTH_WEEKDAY_HEIGHT = 14;
export const monthRowHeight = (height: number) =>
  Math.min(30, Math.max(16, Math.floor((height - MONTH_WEEKDAY_HEIGHT) / 6)));

/**
 * What to read: the coming week, or a shown month's 42-day grid, which for
 * the current month reaches at least a week ahead (readInterval).
 */
export function readInterval(month: number | null, now: number, firstWeekday: number) {
  const today = startOfDay(now);
  const weekEnd = addDays(today, 7);
  if (month === null) return { start: today, end: weekEnd };
  const days = monthDays(month, firstWeekday);
  const end = addDays(days[41]!, 1);
  return { start: days[0]!, end: sameMonth(month, now) ? Math.max(end, weekEnd) : end };
}

// MARK: Events

/** Each event once, with a real span, by start, end and id (ordered). */
export function ordered(events: CalendarEvent[]): CalendarEvent[] {
  const seen = new Set<string>();
  return events
    .filter((e) => {
      if (!Number.isFinite(e.start) || !Number.isFinite(e.end) || e.end <= e.start || seen.has(e.id)) return false;
      seen.add(e.id);
      return true;
    })
    .sort((a, b) => a.start - b.start || a.end - b.end || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

export const upcoming = (events: CalendarEvent[], now: number) => ordered(events).filter((e) => e.end > now);

/** The first timed event not yet over: the agenda's "Up next". */
export const next = (events: CalendarEvent[], now: number) => upcoming(events, now).find((e) => !e.allDay) ?? null;

/** Events touching `day`, all-day first; ends are exclusive. */
export function eventsOn(events: CalendarEvent[], day: number): CalendarEvent[] {
  const start = startOfDay(day);
  const end = addDays(start, 1);
  return events
    .filter((e) => e.start < end && e.end > start)
    .sort(
      (a, b) => Number(b.allDay) - Number(a.allDay) || a.start - b.start || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
    );
}

/** Up to three colours under a day's number, each once. */
export function dotColors(events: CalendarEvent[]): string[] {
  return [...new Set(events.map((e) => e.color))].slice(0, 3);
}

export interface DayGroup {
  day: number;
  events: CalendarEvent[];
}

/** The agenda: the chosen day, or each of the next seven days still to come, skipping empty ones. */
export function agendaGroups(events: CalendarEvent[], now: number, selectedDay: number | null): DayGroup[] {
  const today = startOfDay(now);
  const days = selectedDay === null ? Array.from({ length: 7 }, (_, i) => addDays(today, i)) : [selectedDay];
  const list = selectedDay === null ? upcoming(events, now) : events;
  return days.map((day) => ({ day, events: eventsOn(list, day) })).filter((g) => g.events.length > 0);
}

export const displayTitle = (event: CalendarEvent, untitled: string) => event.title.trim() || untitled;

// MARK: Countdowns

export interface Countdown {
  event: CalendarEvent;
  /** Counting to the event's end while it happens, else to its start. */
  ongoing: boolean;
}

export const countdownTarget = (c: Countdown) => (c.ongoing ? c.event.end : c.event.start);

/** Each moment shows during the hour before it; an end only once its event has begun. */
export function isShown(c: Countdown, now: number): boolean {
  const target = countdownTarget(c);
  return target > now && target - now <= LEAD_TIME && (!c.ongoing || c.event.start <= now);
}

export interface Follow {
  /** Every timed event's start (calendarCountdown). */
  starts: boolean;
  /** The end of the one under way (calendarTimeLeft). */
  ends: boolean;
  /** Countdown keys chosen one by one from an event's menu. */
  chosen: ReadonlySet<string>;
}

export const isChosen = (event: CalendarEvent, chosen: ReadonlySet<string>) =>
  event.countdownKey !== "" && chosen.has(event.countdownKey);

/** The moments followed for one event: its start (false) and its end (true). */
function followed(event: CalendarEvent, follow: Follow): boolean[] {
  return [...(follow.starts || isChosen(event, follow.chosen) ? [false] : []), ...(follow.ends ? [true] : [])];
}

/**
 * What the closed island counts down to: the nearest followed moment within
 * its hour. A start that coincides with an end leaves the event under way.
 */
export function countdown(events: CalendarEvent[], now: number, follow: Follow): Countdown | null {
  let best: Countdown | null = null;
  for (const event of ordered(events)) {
    if (event.allDay) continue;
    for (const ongoing of followed(event, follow)) {
      const candidate = { event, ongoing };
      if (!isShown(candidate, now)) continue;
      const t = countdownTarget(candidate);
      const bestTarget = best ? countdownTarget(best) : Infinity;
      if (t < bestTarget || (t === bestTarget && ongoing && !best!.ongoing)) best = candidate;
    }
  }
  return best;
}

/** When the next followed hour opens or closes, so the island looks again then. */
export function countdownTransition(events: CalendarEvent[], now: number, follow: Follow): number | null {
  const moments = ordered(events)
    .filter((e) => !e.allDay)
    .flatMap((e) =>
      followed(e, follow).flatMap((ongoing) =>
        ongoing ? [Math.max(e.start, e.end - LEAD_TIME), e.end] : [e.start - LEAD_TIME, e.start],
      ),
    )
    .filter((t) => t > now);
  return moments.length ? Math.min(...moments) : null;
}

/** `m:ss` to the moment, rounded up so the last second reads 0:01. */
export function countdownText(target: number, now: number): string {
  const seconds = Math.max(0, Math.ceil((target - now) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

const clock = (t: number, locale?: string) =>
  dateTimeFormat({ hour: "2-digit", minute: "2-digit" }, locale).format(new Date(t));

/** Beside the clock: `· 14:00` for a start, `→ 15:00` for an end (thin space). */
export const timeText = (c: Countdown, locale?: string) =>
  `${c.ongoing ? "→" : "·"} ${clock(countdownTarget(c), locale)}`;

/** An event card's time line: start · end on one day, else both dates with an arrow. */
export function timeLine(event: CalendarEvent, day: number, allDay: string, locale?: string): string {
  if (event.allDay) return allDay;
  if (sameDay(event.start, day) && sameDay(event.end - 1000, day)) {
    return `${clock(event.start, locale)} · ${clock(event.end, locale)}`;
  }
  const dated = dateTimeFormat({ day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }, locale);
  return `${dated.format(new Date(event.start))} → ${dated.format(new Date(event.end))}`;
}

/**
 * The Controls tile: the next timed start within the coming week, not only
 * in the countdown's hour. One under way is known already (tileEvent).
 */
export function tileEvent(events: CalendarEvent[], now: number): CalendarEvent | null {
  const { end } = readInterval(null, now, 0);
  return ordered(events).find((e) => !e.allDay && e.start > now && e.start < end) ?? null;
}

/** A start later today reads as its time; a later day adds its weekday (tileStartText). */
export function tileStartText(start: number, now: number, locale?: string): string {
  const options: Intl.DateTimeFormatOptions = sameDay(start, now)
    ? { hour: "2-digit", minute: "2-digit" }
    : { weekday: "short", hour: "2-digit", minute: "2-digit" };
  return dateTimeFormat(options, locale).format(new Date(start));
}

// MARK: Chosen countdowns

/**
 * The chosen countdowns after a read: an event read again keeps its current
 * end, so a moved event stays chosen, and a choice whose event has ended is
 * forgotten. Null when nothing changes, so a read writes no preference.
 */
export function refreshedChoices(
  choices: Record<string, number>,
  events: CalendarEvent[],
  now: number,
): Record<string, number> | null {
  const refreshed: Record<string, number> = { ...choices };
  for (const e of events) if (e.countdownKey in refreshed) refreshed[e.countdownKey] = e.end;
  for (const key of Object.keys(refreshed)) if (!(refreshed[key]! > now)) delete refreshed[key];
  const same =
    Object.keys(refreshed).length === Object.keys(choices).length &&
    Object.entries(refreshed).every(([k, v]) => choices[k] === v);
  return same ? null : refreshed;
}

// MARK: Settings

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Accounts in name order, each with its calendars in name order (grouped). */
export function groupedChoices(choices: CalendarChoice[]): CalendarChoice[][] {
  const groups = new Map<string, CalendarChoice[]>();
  for (const c of choices) groups.set(c.sourceId, [...(groups.get(c.sourceId) ?? []), c]);
  return [...groups.values()]
    .map((g) => g.sort((a, b) => byName.compare(a.title, b.title)))
    .sort(
      (a, b) =>
        byName.compare(a[0]!.source, b[0]!.source) ||
        (a[0]!.sourceId < b[0]!.sourceId ? -1 : a[0]!.sourceId > b[0]!.sourceId ? 1 : 0),
    );
}
