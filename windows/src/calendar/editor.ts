// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { addDays, startOfDay } from "./model";
import type { CalendarEvent, EventDraft } from "./types";

/** datetime-local values must use local wall time, never UTC's ISO date. */
export function inputDate(ms: number, allDay: boolean) {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  return allDay ? date : `${date}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function inputTime(value: string, allDay: boolean): number {
  return new Date(allDay ? `${value}T00:00` : value).getTime();
}

export function initialDraft(event: CalendarEvent | null, day: number, now = Date.now()): EventDraft {
  if (event) return { ...event, description: event.description ?? "" };
  const soon = new Date(Math.ceil((now + 60_000) / 1_800_000) * 1_800_000);
  const start = new Date(day);
  start.setHours(soon.getHours(), soon.getMinutes(), 0, 0);
  return {
    id: null,
    title: "",
    start: start.getTime(),
    end: start.getTime() + 3_600_000,
    allDay: false,
    location: "",
    description: "",
    color: "#ff5c63",
  };
}

export function toggleAllDay(draft: EventDraft, allDay: boolean): EventDraft {
  if (allDay) return { ...draft, allDay, start: startOfDay(draft.start), end: addDays(startOfDay(draft.end - 1), 1) };
  const start = new Date(draft.start);
  start.setHours(9, 0, 0, 0);
  const end = new Date(draft.end);
  end.setDate(end.getDate() - 1);
  end.setHours(10, 0, 0, 0);
  return { ...draft, allDay, start: start.getTime(), end: end.getTime() };
}

/** All-day ends are exclusive on disk, inclusive in the editor. */
export function displayedEnd(draft: EventDraft) {
  return inputDate(draft.allDay ? draft.end - 1 : draft.end, draft.allDay);
}
export function parseEnd(value: string, allDay: boolean) {
  const date = inputTime(value, allDay);
  return allDay ? addDays(date, 1) : date;
}
