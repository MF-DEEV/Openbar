// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The calendar as the island knows it (NotchCalendarService.swift): what
// calendar.rs last read, the countdown the closed island follows, and the
// page's own place in time, which its top-row title and arrows share.

import { create } from "zustand";
import { modulesOf, useIsland } from "../island/store";
import type { IslandSettings } from "../island/settings";
import { bridge, inTauri } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import {
  addDays,
  addMonths,
  type Countdown,
  countdown,
  type Follow,
  readInterval,
  refreshedChoices,
  startOfDay,
} from "./model";
import type { CalendarEvent, CalendarSnapshot } from "./types";

export interface CalendarState extends CalendarSnapshot {
  /** A first answer from calendar.rs has arrived. */
  received: boolean;
  /** The page asked for other days and they have not arrived yet. */
  loading: boolean;
  requesting: boolean;
  requestFailed: boolean;
  /** What the closed island counts down to now. */
  countdown: Countdown | null;
  /** The last countdown shown, which the strip keeps drawing while it leaves. */
  lastCountdown: Countdown | null;
  /** The event the page scrolls to once, after a click on the countdown. */
  revealing: string | null;

  // The page's place in time (NotchCalendarView's @State).
  /** The day the month grid or the week strip is built around. */
  focus: number;
  selectedDay: number | null;
  /** The short island's month grid, over the strip and the cards. */
  showingMonth: boolean;
  /** The page is tall and wide enough for the month beside the agenda. */
  wide: boolean;
  /** The page's width, which the island's top row shares. */
  width: number;
}

const EMPTY: CalendarSnapshot = {
  access: inTauri ? "allowed" : "unavailable",
  events: [],
  current: [],
  calendars: [],
  firstWeekday: 1,
  minimumDaysInFirstWeek: 4,
};

export const useCalendar = create<CalendarState>(() => ({
  ...EMPTY,
  received: !inTauri,
  loading: false,
  requesting: false,
  requestFailed: false,
  countdown: null,
  lastCountdown: null,
  revealing: null,
  focus: Date.now(),
  selectedDay: null,
  showingMonth: false,
  wide: false,
  width: 0,
}));

/** The section shows anywhere only while it is on in Settings › Content. */
export function calendarShown(): boolean {
  const island = useIsland.getState();
  return island.settings.enabled && modulesOf(island).includes("calendar");
}

export function followOf(settings: IslandSettings): Follow {
  return {
    starts: settings.calendarCountdown,
    ends: settings.calendarTimeLeft,
    chosen: new Set(Object.keys(settings.calendarChosenCountdowns)),
  };
}

/** The countdown the closed island shows now, if any. */
export function currentCountdown(now = Date.now()): Countdown | null {
  const { access, current } = useCalendar.getState();
  if (!calendarShown() || access !== "allowed") return null;
  return countdown(current, now, followOf(useIsland.getState().settings));
}

/** Whether the event counted down to began just now, as NotchMascotSupport.eventBegan: a PC that slept through the start, or an event moved or removed before it, is no news. */
export function countdownStarted(before: Countdown | null, after: Countdown | null, now: number): boolean {
  if (!before || before.ongoing) return false;
  const since = (now - before.event.start) / 1000;
  if (since < -2 || since >= 60) return false;
  return !after || after.event.id !== before.event.id || after.ongoing;
}

export function receive(snapshot: CalendarSnapshot) {
  useCalendar.setState({ ...snapshot, received: true, loading: false });
  if (snapshot.access === "allowed") forgetEndedChoices(snapshot.current);
}

/** A chosen event read again keeps its end; one that has ended is forgotten (updateChosenCountdowns). */
function forgetEndedChoices(current: CalendarEvent[]) {
  const choices = useIsland.getState().settings.calendarChosenCountdowns;
  if (Object.keys(choices).length === 0) return;
  const refreshed = refreshedChoices(choices, current, Date.now());
  if (refreshed) void setPrefs({ island: { calendarChosenCountdowns: choicePatch(choices, refreshed) } });
}

/** Prefs merge objects, so a key leaves only as null. */
function choicePatch(before: Record<string, number>, after: Record<string, number>) {
  const patch: Record<string, number | null> = { ...after };
  for (const key of Object.keys(before)) if (!(key in after)) patch[key] = null;
  return patch as Record<string, number>;
}

/** Counts down to one event, or stops, whatever the countdown for every event does. */
export function setCountdownChoice(event: CalendarEvent, chosen: boolean) {
  if (!event.countdownKey) return;
  const choices = useIsland.getState().settings.calendarChosenCountdowns;
  const after = { ...choices };
  if (chosen) after[event.countdownKey] = event.end;
  else delete after[event.countdownKey];
  void setPrefs({ island: { calendarChosenCountdowns: choicePatch(choices, after) } });
}

export async function subscribeCalendar() {
  // Listen first: an initial answer must not overwrite a newer event.
  let version = 0;
  await bridge.calendar.onSnapshot((snapshot) => {
    version++;
    receive(snapshot);
  });
  const at = version;
  const first = await bridge.calendar.snapshot();
  if (first && at === version) receive(first);
}

/** The page reads its month's grid while open, and the coming week alone once closed (showMonth). */
let shownRequest = 0;
export async function showMonth(month: number | null) {
  const request = ++shownRequest;
  const { firstWeekday } = useCalendar.getState();
  // Outside Tauri nothing answers, so a designed sample stays.
  if (month !== null && inTauri) useCalendar.setState({ events: [], loading: true });
  try {
    const answer = await bridge.calendar.showRange(
      month === null ? null : readInterval(month, Date.now(), firstWeekday),
    );
    // A later request's answer wins, whichever arrives first.
    if (answer && request === shownRequest) receive(answer);
  } finally {
    if (request === shownRequest) useCalendar.setState({ loading: false });
  }
}

export async function requestAccess() {
  useCalendar.setState({ requesting: true, requestFailed: false });
  try {
    const access = await bridge.calendar.requestAccess();
    // Still unasked after asking: Windows showed no prompt, so Settings is the way.
    useCalendar.setState({
      requesting: false,
      requestFailed: inTauri && (access === undefined || access === "notAsked"),
    });
    const answer = await bridge.calendar.snapshot();
    if (answer) receive(answer);
  } catch {
    useCalendar.setState({ requesting: false, requestFailed: true });
  }
}

// MARK: The page's place in time

export const calendarView = {
  select(day: number) {
    useCalendar.setState({ selectedDay: startOfDay(day), focus: day });
  },
  /** Back to the coming week. */
  week() {
    useCalendar.setState({ selectedDay: null, focus: Date.now() });
  },
  moveWeek(offset: number) {
    const date = addDays(useCalendar.getState().focus, offset * 7);
    useCalendar.setState({ selectedDay: startOfDay(date), focus: date });
  },
  moveMonth(offset: number) {
    const date = addMonths(useCalendar.getState().focus, offset);
    useCalendar.setState({ selectedDay: date, focus: date });
  },
  today() {
    const now = Date.now();
    useCalendar.setState({ selectedDay: startOfDay(now), focus: now, showingMonth: false });
  },
  toggleMonth() {
    useCalendar.setState((s) => ({ showingMonth: !s.showingMonth }));
  },
  closeMonth() {
    useCalendar.setState({ showingMonth: false });
  },
  /** A page opened again starts on the coming week, as the Mac's view state does. */
  reset() {
    useCalendar.setState({ focus: Date.now(), selectedDay: null, showingMonth: false });
  },
};
