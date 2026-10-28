// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
import type { CalendarEvent, CalendarSnapshot } from "./types";

const connection = vi.hoisted(() => ({ receive: (_: CalendarSnapshot) => {} }));
vi.mock("../shared/bridge", () => ({
  inTauri: false,
  bridge: {
    calendar: {
      onSnapshot: async (handler: (snapshot: CalendarSnapshot) => void) => {
        connection.receive = handler;
        return () => {};
      },
      snapshot: async () => undefined,
      showRange: async () => undefined,
    },
  },
}));

import { DEFAULT_SETTINGS } from "../island/settings";
import { useIsland } from "../island/store";
import { loadPrefs } from "../shared/prefs";
import "./register";
import { setCountdownChoice, useCalendar } from "./store";

const MIN = 60_000;
function event(start: number, end: number, key = "standup"): CalendarEvent {
  return {
    id: `${key}:${start}`,
    title: "Standup",
    calendar: "Work",
    calendarId: "work",
    start,
    end,
    allDay: false,
    location: "",
    color: "#ff9f0a",
    localId: key,
    instanceStart: null,
    countdownKey: key,
  };
}
const snapshot = (current: CalendarEvent[]): CalendarSnapshot => ({
  access: "allowed",
  events: current,
  current,
  calendars: [],
  firstWeekday: 1,
  minimumDaysInFirstWeek: 4,
});

afterEach(() => {
  vi.useRealTimers();
  useIsland.setState({ settings: DEFAULT_SETTINGS, activities: {} });
  connection.receive(snapshot([]));
});

describe("calendar activity", () => {
  it("takes the closed island in the lead window only with the countdown on", () => {
    const soon = event(Date.now() + 20 * MIN, Date.now() + 40 * MIN);
    connection.receive(snapshot([soon]));
    expect(useIsland.getState().activities.calendar).toBeUndefined();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, calendarCountdown: true } });
    expect(useIsland.getState().activities.calendar?.opens).toBe("calendar");
    expect(useCalendar.getState().countdown?.event.id).toBe(soon.id);
    // Hidden in Settings › Content, the section shows nowhere.
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, calendarCountdown: true, hiddenModules: ["calendar"] } });
    expect(useIsland.getState().activities.calendar).toBeUndefined();
  });

  it("appears when the lead window opens and leaves when the event starts", () => {
    vi.useFakeTimers();
    const start = Date.now() + 61 * MIN;
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, calendarCountdown: true } });
    connection.receive(snapshot([event(start, start + 30 * MIN)]));
    expect(useIsland.getState().activities.calendar).toBeUndefined();
    vi.advanceTimersByTime(MIN + 100);
    expect(useIsland.getState().activities.calendar).toBeDefined();
    vi.advanceTimersByTime(60 * MIN);
    expect(useIsland.getState().activities.calendar).toBeUndefined();
    // The strip keeps drawing the last countdown while it leaves.
    expect(useCalendar.getState().lastCountdown?.event.start).toBe(start);
  });

  it("persists a chosen countdown until its event ends", async () => {
    const chosen = event(Date.now() + 30 * MIN, Date.now() + 50 * MIN, "review");
    connection.receive(snapshot([chosen]));
    setCountdownChoice(chosen, true);
    const prefs = await loadPrefs();
    expect(prefs.island.calendarChosenCountdowns).toEqual({ review: chosen.end });
    useIsland.setState({ settings: prefs.island });
    expect(useCalendar.getState().countdown?.event.id).toBe(chosen.id);
    // A read after the event ended forgets it.
    useIsland.setState({ settings: { ...prefs.island, calendarChosenCountdowns: { review: Date.now() - 1 } } });
    connection.receive(snapshot([]));
    await vi.waitFor(async () => expect((await loadPrefs()).island.calendarChosenCountdowns).toEqual({}));
  });
});
