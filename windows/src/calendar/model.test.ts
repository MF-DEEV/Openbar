// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  addDays,
  agendaGroups,
  countdown,
  countdownText,
  countdownTransition,
  dotColors,
  eventsOn,
  type Follow,
  groupedChoices,
  LEAD_TIME,
  monthDays,
  monthRowHeight,
  next,
  readInterval,
  refreshedChoices,
  startsWeek,
  tileEvent,
  tileStartText,
  timeLine,
  timeText,
  weekDays,
  weekNumber,
} from "./model";
import type { CalendarEvent } from "./types";

const MIN = 60_000;
const at = (y: number, m: number, d: number, h = 0, min = 0) => new Date(y, m - 1, d, h, min).getTime();
let n = 0;
function event(start: number, end: number, extra: Partial<CalendarEvent> = {}): CalendarEvent {
  const id = `e${n++}`;
  return {
    id,
    title: id,
    calendar: "Work",
    calendarId: "work",
    start,
    end,
    allDay: false,
    location: "",
    color: "#ff0000",
    localId: id,
    instanceStart: null,
    countdownKey: id,
    ...extra,
  };
}
const follow = (f: Partial<Follow> = {}): Follow => ({ starts: true, ends: false, chosen: new Set(), ...f });
const now = at(2026, 10, 27, 10, 0);

describe("countdown target", () => {
  it("shows a start only within the 60-minute lead window", () => {
    const later = event(now + LEAD_TIME + MIN, now + 2 * LEAD_TIME);
    const soon = event(now + LEAD_TIME, now + 2 * LEAD_TIME);
    expect(countdown([later], now, follow())).toBeNull();
    expect(countdown([soon], now, follow())?.event).toBe(soon);
    expect(countdown([event(now, now + MIN)], now, follow())).toBeNull();
  });

  it("lets the nearest moment win", () => {
    const a = event(now + 30 * MIN, now + 90 * MIN);
    const b = event(now + 10 * MIN, now + 20 * MIN);
    expect(countdown([a, b], now, follow())?.event).toBe(b);
  });

  it("counts an ongoing event to its end only with time left on, and only once it began", () => {
    const meeting = event(now - 10 * MIN, now + 20 * MIN);
    expect(countdown([meeting], now, follow())).toBeNull();
    expect(countdown([meeting], now, follow({ ends: true }))).toEqual({ event: meeting, ongoing: true });
    const notYet = event(now + 5 * MIN, now + 20 * MIN);
    expect(countdown([notYet], now, follow({ starts: false, ends: true }))).toBeNull();
    // An end more than an hour away waits.
    expect(countdown([event(now - MIN, now + 2 * LEAD_TIME)], now, follow({ ends: true }))).toBeNull();
  });

  it("leaves the event under way when its end meets the next start", () => {
    const first = event(now - 30 * MIN, now + 15 * MIN);
    const second = event(now + 15 * MIN, now + 45 * MIN);
    expect(countdown([second, first], now, follow({ ends: true }))).toEqual({ event: first, ongoing: true });
  });

  it("follows individually chosen events while the global countdown is off", () => {
    const chosen = event(now + 20 * MIN, now + 40 * MIN);
    const other = event(now + 10 * MIN, now + 40 * MIN);
    expect(countdown([chosen, other], now, follow({ starts: false }))).toBeNull();
    const result = countdown([chosen, other], now, follow({ starts: false, chosen: new Set([chosen.countdownKey]) }));
    expect(result?.event).toBe(chosen);
  });

  it("ignores all-day events", () => {
    expect(countdown([event(now + MIN, now + LEAD_TIME, { allDay: true })], now, follow())).toBeNull();
  });

  it("knows when the next lead window opens or closes", () => {
    const e = event(now + 90 * MIN, now + 120 * MIN);
    expect(countdownTransition([e], now, follow())).toBe(now + 30 * MIN);
    expect(countdownTransition([e], now + 40 * MIN, follow())).toBe(now + 90 * MIN);
    expect(countdownTransition([e], now, follow({ starts: false }))).toBeNull();
    // An end's hour opens no earlier than the start.
    expect(countdownTransition([e], now + 91 * MIN, follow({ starts: false, ends: true }))).toBe(now + 120 * MIN);
  });
});

describe("compact text", () => {
  it("rounds up to m:ss", () => {
    expect(countdownText(now + 59 * MIN + 1, now)).toBe("59:01");
    expect(countdownText(now + 1500, now)).toBe("0:02");
    expect(countdownText(now - 5000, now)).toBe("0:00");
    expect(countdownText(now + 60 * MIN, now)).toBe("60:00");
  });

  it("marks a start with a dot and an end with an arrow", () => {
    const e = event(at(2026, 10, 27, 14, 0), at(2026, 10, 27, 15, 30));
    expect(timeText({ event: e, ongoing: false }, "en-GB")).toBe("· 14:00");
    expect(timeText({ event: e, ongoing: true }, "en-GB")).toBe("→ 15:30");
  });

  it("writes the card's time line for one day, across days and all day", () => {
    const day = at(2026, 10, 27);
    expect(timeLine(event(at(2026, 10, 27, 9), at(2026, 10, 27, 10)), day, "All day", "en-GB")).toBe("09:00 · 10:00");
    // Ending at midnight still reads as one day.
    expect(timeLine(event(at(2026, 10, 27, 23), at(2026, 10, 28)), day, "All day", "en-GB")).toBe("23:00 · 00:00");
    expect(timeLine(event(at(2026, 10, 27, 22), at(2026, 10, 28, 2)), day, "All day", "en-GB")).toBe(
      "27 Oct, 22:00 → 28 Oct, 02:00",
    );
    expect(timeLine(event(day, addDays(day, 1), { allDay: true }), day, "All day", "en-GB")).toBe("All day");
  });

  it("names the tile's next start: the time today, the weekday later", () => {
    const today = event(at(2026, 10, 27, 15), at(2026, 10, 27, 16));
    const friday = event(at(2026, 10, 30, 9), at(2026, 10, 30, 10));
    const farAway = event(at(2026, 11, 5, 9), at(2026, 11, 5, 10));
    const ongoing = event(now - MIN, now + MIN);
    expect(tileEvent([farAway, friday, ongoing], now)).toBe(friday);
    expect(tileEvent([friday, today], now)).toBe(today);
    expect(tileEvent([farAway], now)).toBeNull();
    expect(tileStartText(today.start, now, "en-GB")).toBe("15:00");
    expect(tileStartText(friday.start, now, "en-GB")).toBe("Fri 09:00");
  });
});

describe("grid math", () => {
  it("starts the 42-day grid on the locale's first weekday", () => {
    const monday = monthDays(at(2026, 10, 15), 1);
    expect(monday).toHaveLength(42);
    expect(monday[0]).toBe(at(2026, 9, 28));
    expect(monday[41]).toBe(at(2026, 11, 8));
    const sunday = monthDays(at(2026, 10, 15), 0);
    expect(sunday[0]).toBe(at(2026, 9, 27));
    // A month starting on the first weekday starts the grid with its first.
    expect(monthDays(at(2026, 6, 10), 1)[0]).toBe(at(2026, 6, 1));
  });

  it("keeps every week inside its month's grid, across daylight saving", () => {
    const week = weekDays(at(2026, 10, 27, 15), 1);
    expect(week[0]).toBe(at(2026, 10, 26));
    expect(week[6]).toBe(at(2026, 11, 1));
    // The European clock change on 25 Oct 2026 still gives local midnights.
    const dst = weekDays(at(2026, 10, 25, 12), 1);
    expect(dst.map((d) => new Date(d).getHours())).toEqual([0, 0, 0, 0, 0, 0, 0]);
    expect(startsWeek(week[0]!, 1)).toBe(true);
    expect(startsWeek(week[1]!, 1)).toBe(false);
  });

  it("numbers weeks by the locale's rules", () => {
    // ISO (Monday, four days): 1 Jan 2026 is a Thursday, so week 1 holds it.
    expect(weekNumber(at(2025, 12, 29), 1, 4)).toBe(1);
    expect(weekNumber(at(2026, 10, 26), 1, 4)).toBe(44);
    // 1 Jan 2027 is a Friday: still week 53 of 2026.
    expect(weekNumber(at(2026, 12, 28), 1, 4)).toBe(53);
    expect(weekNumber(at(2027, 1, 4), 1, 4)).toBe(1);
    // US (Sunday, one day): the week holding 1 Jan is week 1.
    expect(weekNumber(at(2026, 12, 27), 0, 1)).toBe(1);
    expect(weekNumber(at(2026, 10, 25), 0, 1)).toBe(44);
    // First full week (seven days): 1 Jan 2026 on a Thursday is week 53 of 2025.
    expect(weekNumber(at(2025, 12, 29), 1, 7)).toBe(52);
    expect(weekNumber(at(2026, 1, 5), 1, 7)).toBe(1);
  });

  it("sizes the short island's month rows between 16 and 30", () => {
    expect(monthRowHeight(500)).toBe(30);
    expect(monthRowHeight(14 + 6 * 22 + 3)).toBe(22);
    expect(monthRowHeight(50)).toBe(16);
  });

  it("reads the coming week, or a month's grid that reaches a week ahead for this month", () => {
    expect(readInterval(null, now, 1)).toEqual({ start: at(2026, 10, 27), end: at(2026, 11, 3) });
    expect(readInterval(at(2026, 10, 1), now, 1)).toEqual({ start: at(2026, 9, 28), end: at(2026, 11, 9) });
    const late = at(2026, 11, 7, 12);
    // November's grid ends 6 Dec; the week from 7 Nov is inside it.
    expect(readInterval(at(2026, 11, 1), late, 1).end).toBe(at(2026, 12, 7));
    const lateOctober = at(2026, 10, 31, 12);
    expect(readInterval(at(2026, 10, 1), lateOctober, 1).end).toBe(at(2026, 11, 9));
  });
});

describe("agenda", () => {
  const today = at(2026, 10, 27);
  const done = event(at(2026, 10, 27, 8), at(2026, 10, 27, 9));
  const later = event(at(2026, 10, 27, 15), at(2026, 10, 27, 16));
  const allDay = event(today, addDays(today, 1), { allDay: true });
  const tomorrow = event(at(2026, 10, 28, 9), at(2026, 10, 28, 10));
  const nextWeek = event(at(2026, 11, 4, 9), at(2026, 11, 4, 10));
  const twoDays = event(at(2026, 10, 29, 22), at(2026, 10, 30, 2));

  it("groups the coming week by day, leaving out what has ended and empty days", () => {
    const groups = agendaGroups([nextWeek, tomorrow, later, done, allDay, twoDays], now, null);
    expect(groups.map((g) => g.day)).toEqual([today, at(2026, 10, 28), at(2026, 10, 29), at(2026, 10, 30)]);
    expect(groups[0]!.events).toEqual([allDay, later]);
    expect(groups[3]!.events).toEqual([twoDays]);
  });

  it("shows everything on a chosen day, ended events included", () => {
    expect(agendaGroups([done, later, tomorrow], now, today)).toEqual([{ day: today, events: [done, later] }]);
    expect(agendaGroups([done], now, at(2026, 10, 30))).toEqual([]);
  });

  it("marks the first timed event not yet over as next", () => {
    expect(next([allDay, later, done], now)).toBe(later);
  });

  it("puts all-day events first and treats ends as exclusive", () => {
    const midnight = event(at(2026, 10, 26, 22), today);
    expect(eventsOn([later, allDay, midnight], today)).toEqual([allDay, later]);
  });

  it("draws up to three distinct colours", () => {
    const colors = ["#1", "#2", "#1", "#3", "#4"].map((color) => event(now, now + MIN, { color }));
    expect(dotColors(colors)).toEqual(["#1", "#2", "#3"]);
  });
});

describe("chosen countdowns", () => {
  it("keeps a moved event's new end and forgets ended ones", () => {
    const moved = event(now + 10 * MIN, now + 50 * MIN);
    const choices = { [moved.countdownKey]: now + 30 * MIN, gone: now - MIN, later: now + 9e9 };
    expect(refreshedChoices(choices, [moved], now)).toEqual({ [moved.countdownKey]: now + 50 * MIN, later: now + 9e9 });
    expect(refreshedChoices({ later: now + 9e9 }, [moved], now)).toBeNull();
  });
});

describe("calendar choices", () => {
  it("groups calendars by account, both in name order", () => {
    const c = (id: string, title: string, sourceId: string, source: string) => ({
      id,
      title,
      sourceId,
      source,
      color: "#fff",
    });
    const groups = groupedChoices([
      c("1", "Work", "b", "Outlook"),
      c("2", "Birthdays", "a", "Gmail"),
      c("3", "Calendar 10", "b", "Outlook"),
      c("4", "Calendar 9", "b", "Outlook"),
    ]);
    expect(groups.map((g) => g.map((x) => x.id))).toEqual([["2"], ["4", "3", "1"]]);
  });
});
