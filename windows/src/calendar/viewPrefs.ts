// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
/** View preferences only. Event data belongs to the native on-disk store. */
export interface CalendarPlace {
  focus: number;
  selectedDay: number | null;
  showingMonth: boolean;
}
const KEY = "openbar.calendar.place.v1";
export function restoredPlace(now = Date.now()): CalendarPlace {
  const fallback = { focus: now, selectedDay: null, showingMonth: true };
  try {
    const place = JSON.parse(localStorage.getItem(KEY) ?? "null");
    const valid = (n: unknown): n is number =>
      typeof n === "number" && Number.isFinite(n) && n >= 0 && n < 7_258_118_400_000;
    if (
      !place ||
      !valid(place.focus) ||
      (place.selectedDay !== null && !valid(place.selectedDay)) ||
      typeof place.showingMonth !== "boolean"
    )
      return fallback;
    return { focus: place.focus, selectedDay: place.selectedDay, showingMonth: place.showingMonth };
  } catch {
    return fallback;
  }
}
export function rememberPlace(place: CalendarPlace) {
  try {
    localStorage.setItem(KEY, JSON.stringify(place));
  } catch {
    /* A full/disabled web storage must not affect saved events. */
  }
}
