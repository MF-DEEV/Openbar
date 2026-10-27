// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import * as m from "./model";

const config = (over: Partial<m.PomodoroConfig> = {}) => m.pomodoroConfig(over);

describe("pomodoro configuration", () => {
  it("clamps every value to the Mac's ranges", () => {
    expect(
      config({ focusMinutes: 0, shortBreakMinutes: 99, longBreakMinutes: -3, longBreakInterval: 30, totalSessions: 0 }),
    ).toEqual({ focusMinutes: 1, shortBreakMinutes: 60, longBreakMinutes: 1, longBreakInterval: 24, totalSessions: 1 });
    expect(config({ focusMinutes: 500 }).focusMinutes).toBe(180);
    expect(config({ focusMinutes: NaN }).focusMinutes).toBe(25);
    expect(config({ shortBreakMinutes: 4.6 }).shortBreakMinutes).toBe(5);
  });

  it("clamps the timer's minutes to 1…180", () => {
    expect(m.timerMinutes(0)).toBe(1);
    expect(m.timerMinutes(181)).toBe(180);
    expect(m.timerMinutes(Infinity)).toBe(15);
  });
});

describe("sessions", () => {
  it("counts a timer down from its deadline and completes it once", () => {
    const s = m.start(m.IDLE, "timer", 5, 100);
    expect(m.isRunning(s)).toBe(true);
    expect(m.deadline(s)).toBe(400);
    expect(m.reading(s, 250)).toBe(150);
    expect(m.finishIfDue(s, 399.99)).toBeNull();
    const done = m.finishIfDue(s, 400)!;
    expect(done.completed).toBe(true);
    expect(m.isRunning(done)).toBe(false);
    expect(m.reading(done, 9999)).toBe(0);
    expect(m.finishIfDue(done, 500)).toBeNull();
    // A running session can't be started over.
    expect(m.start(s, "stopwatch", 1, 200)).toBe(s);
  });

  it("re-anchors on resume, so a pause never loses or gains time", () => {
    const s = m.start(m.IDLE, "timer", 10, 0);
    const paused = m.pause(s, 120.4);
    expect(m.isPaused(paused)).toBe(true);
    expect(m.reading(paused, 5000)).toBeCloseTo(479.6);
    const resumed = m.resume(paused, 1000);
    expect(m.deadline(resumed)).toBeCloseTo(1479.6);
    expect(m.reading(resumed, 1100)).toBeCloseTo(379.6);
  });

  it("pauses and resumes a stopwatch from its elapsed time", () => {
    const s = m.start(m.IDLE, "stopwatch", 0, 50);
    expect(m.deadline(s)).toBeNull();
    expect(m.finishIfDue(s, 1e9)).toBeNull();
    const paused = m.pause(s, 80);
    expect(m.reading(paused, 999)).toBe(30);
    const resumed = m.resume(paused, 200);
    expect(m.reading(resumed, 210)).toBe(40);
  });

  it("completes instead of pausing a countdown already due", () => {
    const s = m.start(m.IDLE, "timer", 1, 0);
    const paused = m.pause(s, 61);
    expect(paused.completed).toBe(true);
    expect(m.isPaused(paused)).toBe(false);
  });

  it("returns to setup on cancel", () => {
    const s = m.start(m.IDLE, "pomodoro", 0, 0, config());
    expect(m.hasSession(s)).toBe(true);
    expect(m.hasSession(m.IDLE)).toBe(false);
  });
});

describe("pomodoro phases", () => {
  /** Runs the cycle to its end, finishing each phase and starting the next. */
  function phases(c: m.PomodoroConfig): string[] {
    let s = m.start(m.IDLE, "pomodoro", 0, 0, c);
    let now = 0;
    const seen: string[] = [s.phase];
    for (let i = 0; i < 100; i++) {
      now = m.deadline(s)!;
      s = m.finishIfDue(s, now)!;
      if (!m.canStartNext(s)) break;
      s = m.startNext(s, now);
      seen.push(s.phase);
    }
    expect(m.cycleFinished(s)).toBe(true);
    return seen;
  }

  it("takes the long break every interval and ends after the last focus", () => {
    expect(phases(config({ longBreakInterval: 2, totalSessions: 4 }))).toEqual([
      "focus",
      "shortBreak",
      "focus",
      "longBreak",
      "focus",
      "shortBreak",
      "focus",
    ]);
    expect(phases(config({ totalSessions: 1 }))).toEqual(["focus"]);
  });

  it("starts the next phase only when asked, with that phase's length", () => {
    const c = config({ focusMinutes: 25, shortBreakMinutes: 5 });
    const focus = m.start(m.IDLE, "pomodoro", 99, 0, c);
    expect(focus.duration).toBe(25 * 60);
    expect(m.sessionNumber(focus)).toBe(1);
    // Long after the deadline, it is completed and waits; nothing ran on its own.
    const done = m.finishIfDue(focus, 1e6)!;
    expect(done.completed).toBe(true);
    expect(done.completedFocuses).toBe(1);
    expect(m.sessionNumber(done)).toBe(1);
    expect(m.nextPhase(done)).toBe("shortBreak");
    const rest = m.startNext(done, 2e6);
    expect(rest.phase).toBe("shortBreak");
    expect(m.deadline(rest)).toBe(2e6 + 300);
    expect(m.sessionNumber(rest)).toBe(1);
    // A running phase has no next phase to start.
    expect(m.startNext(rest, 2e6)).toBe(rest);
  });

  it("keeps the configuration it started with", () => {
    const s = m.start(m.IDLE, "pomodoro", 0, 0, config({ totalSessions: 2 }));
    expect(s.config.totalSessions).toBe(2);
  });
});

describe("clock text", () => {
  it("rounds countdowns up and saturates at three hours", () => {
    expect(m.clockText(0)).toBe("00:00");
    expect(m.clockText(0.01)).toBe("00:01");
    expect(m.clockText(59.2)).toBe("01:00");
    expect(m.clockText(899)).toBe("14:59");
    expect(m.clockText(3599.5)).toBe("1:00:00");
    expect(m.clockText(3725)).toBe("1:02:05");
    expect(m.clockText(1e9)).toBe("3:00:00");
    expect(m.clockText(-5)).toBe("00:00");
    expect(m.clockText(NaN)).toBe("00:00");
  });

  it("rounds a stopwatch down and saturates at 99:59:59", () => {
    expect(m.stopwatchText(0.99)).toBe("00:00");
    expect(m.stopwatchText(61.9)).toBe("01:01");
    expect(m.stopwatchText(3600)).toBe("1:00:00");
    expect(m.stopwatchText(1e9)).toBe("99:59:59");
  });

  it("reads compactly in the closed island", () => {
    const at = (remaining: number) => m.compactText(m.start(m.IDLE, "timer", 180, 0), 180 * 60 - remaining);
    expect(at(45)).toBe("45s");
    expect(at(44.2)).toBe("45s");
    expect(at(59.5)).toBe("1m");
    expect(at(900)).toBe("15m");
    expect(at(899)).toBe("14m");
    expect(at(5700)).toBe("1h35");
    expect(at(3600)).toBe("1h00");
    const watch = (elapsed: number) => m.compactText(m.start(m.IDLE, "stopwatch", 0, 0), elapsed);
    expect(watch(754.9)).toBe("12:34");
    expect(watch(5700)).toBe("1h35");
  });

  it("names durations in narrow units", () => {
    expect(m.durationText(5 * 60)).toBe("5m");
    expect(m.durationText(60 * 60)).toBe("1h");
    expect(m.durationText(90 * 60)).toBe("1h 30m");
    expect(m.durationText(30)).toBe("30s");
  });

  it("rolls only what comes before the last colon", () => {
    expect(m.rollingSplit("12:04")).toEqual(["12", ":04"]);
    expect(m.rollingSplit("1:02:04")).toEqual(["1:02", ":04"]);
    expect(m.rollingSplit("15m")).toEqual(["15m", ""]);
    expect(m.readingShape("1h35")).toBe("0h00");
  });

  it("ticks just past each whole second of the reading", () => {
    const countdown = m.start(m.IDLE, "timer", 1, 0);
    expect(m.secondBoundaryOffset(countdown, 0.25)).toBeCloseTo(0.75);
    expect(m.secondBoundaryOffset(countdown, 1)).toBe(1);
    const watch = m.start(m.IDLE, "stopwatch", 0, 0);
    expect(m.secondBoundaryOffset(watch, 3.25)).toBeCloseTo(0.75);
    expect(m.secondBoundaryOffset(watch, 3)).toBe(1);
  });
});

describe("the closed island", () => {
  it("hides a running countdown when asked but always shows a finished one", () => {
    const running = m.start(m.IDLE, "timer", 1, 0);
    const done = m.finishIfDue(running, 60)!;
    expect(m.showsActivity(running, true, false)).toBe(true);
    expect(m.showsActivity(running, true, true)).toBe(false);
    expect(m.showsActivity(done, true, true)).toBe(true);
    expect(m.showsActivity(done, false, false)).toBe(false);
    expect(m.showsActivity(m.IDLE, true, false)).toBe(false);
  });
});

describe("ruler", () => {
  it("moves 14 points a minute, dragging left raising the value", () => {
    expect(m.rulerMoving(15, -14)).toBe(16);
    expect(m.rulerMoving(15, 28)).toBe(13);
    expect(m.rulerMoving(15, -7)).toBe(15.5);
    expect(m.rulerMinute(15.5)).toBe(16);
    expect(m.rulerMinute(15.49)).toBe(15);
  });

  it("stays within 1…180", () => {
    expect(m.rulerMoving(2, 1000)).toBe(1);
    expect(m.rulerMoving(179, -1000)).toBe(180);
    expect(m.rulerMoving(NaN, 0)).toBe(1);
    expect(m.rulerMinute(0)).toBe(1);
  });

  it("places ticks and labels around the selection", () => {
    expect(m.rulerOffset(20, 15)).toBe(70);
    expect(m.rulerOffset(10, 15)).toBe(-70);
    expect(m.rulerLabel(45)).toBe("45");
    expect(m.rulerLabel(65)).toBe("1h05");
    expect(m.rulerLabel(180)).toBe("3h00");
  });
});
