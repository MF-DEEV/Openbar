// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { describe, it, expect } from "vitest";
import { displayedEnd, initialDraft, inputDate, inputTime, parseEnd, toggleAllDay } from "./editor";

describe("local calendar editing", () => {
  it("keeps local wall time through editing instead of shifting it to UTC", () => {
    const start = new Date(2026, 10, 8, 14, 30).getTime();
    expect(inputDate(start, false)).toBe("2026-11-08T14:30");
    expect(inputTime(inputDate(start, false), false)).toBe(start);
  });
  it("stores all-day ends exclusively and displays their last included date", () => {
    const draft = initialDraft(null, new Date(2026, 9, 25).getTime());
    draft.start = new Date(2026, 9, 25, 9).getTime();
    draft.end = new Date(2026, 9, 25, 10).getTime();
    const allDay = toggleAllDay(draft, true);
    expect(inputDate(allDay.start, true)).toBe("2026-10-25");
    expect(displayedEnd(allDay)).toBe("2026-10-25");
    expect(inputDate(allDay.end, true)).toBe("2026-10-26");
    expect(parseEnd("2026-10-25", true)).toBe(allDay.end);
    const timed = toggleAllDay(allDay, false);
    expect(inputDate(timed.end, false)).toBe("2026-10-25T10:00");
  });
});
