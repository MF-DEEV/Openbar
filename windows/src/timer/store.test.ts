// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const calls = vi.hoisted(() => ({ alarm: [] as boolean[], wake: [] as (number | null)[] }));
vi.mock("../shared/bridge", () => ({
  inTauri: false,
  bridge: {
    sound: { alarm: async (_key: string, on: boolean) => void calls.alarm.push(on) },
    timer: {
      now: async () => undefined,
      wake: async (delay: number | null) => void calls.wake.push(delay),
      onDue: async () => () => {},
    },
  },
}));

import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";
import * as model from "./model";
import { timer, useTimer } from "./store";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date", "performance"] });
  calls.alarm.length = 0;
  calls.wake.length = 0;
});
afterEach(() => {
  timer.cancel();
  useIsland.getState().dismissNotice();
  useIsland.setState({ settings: DEFAULT_SETTINGS, activities: {} });
  vi.useRealTimers();
});

const session = () => useTimer.getState().session;

describe("timer service", () => {
  it("finishes on time with an orange notice and the chime", () => {
    timer.start("timer", 1);
    expect(calls.wake.at(-1)).toBeCloseTo(60_000);
    vi.advanceTimersByTime(59_100);
    expect(useTimer.getState().session.completed).toBe(false);
    expect(model.clockTextFor(session(), useTimer.getState().now)).toBe("00:01");
    vi.advanceTimersByTime(1_000);
    expect(session().completed).toBe(true);
    const notice = useIsland.getState().notice!;
    expect(notice).toMatchObject({
      kind: "timer",
      title: "Time is up",
      detail: "Timer",
      priority: 2,
      duration: 6,
      opens: "timer",
    });
    expect(notice.tint).toBe("#ff9f0a");
    expect(calls.alarm).toEqual([true]);
    expect(calls.wake.at(-1)).toBeNull();
    timer.cancel();
    expect(calls.alarm).toEqual([true, false]);
    expect(model.hasSession(session())).toBe(false);
  });

  it("stays quiet with sound off", () => {
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, timerSoundEnabled: false } });
    timer.start("timer", 1);
    vi.advanceTimersByTime(61_000);
    expect(session().completed).toBe(true);
    expect(calls.alarm).toEqual([]);
  });

  it("ticks on whole seconds of the reading", () => {
    timer.start("stopwatch", 0);
    vi.advanceTimersByTime(400);
    const before = useTimer.getState().now;
    vi.advanceTimersByTime(700);
    const after = useTimer.getState().now;
    expect(after - before).toBeGreaterThan(0.6);
    expect(model.stopwatchText(model.reading(session(), after))).toBe("00:01");
  });

  it("pauses without losing time, and resumes from the same reading", () => {
    timer.start("timer", 2);
    vi.advanceTimersByTime(30_000);
    timer.pauseOrResume();
    expect(model.isPaused(session())).toBe(true);
    vi.advanceTimersByTime(600_000);
    expect(session().completed).toBe(false);
    timer.pauseOrResume();
    expect(model.reading(session(), useTimer.getState().now)).toBeCloseTo(90, 0);
    vi.advanceTimersByTime(91_000);
    expect(session().completed).toBe(true);
  });

  it("waits for Start before the next Pomodoro phase, and stops the chime then", () => {
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, pomodoroFocusMinutes: 1, pomodoroShortBreakMinutes: 2 } });
    timer.start("pomodoro", 99);
    vi.advanceTimersByTime(61_000);
    expect(session().completed).toBe(true);
    expect(useIsland.getState().notice?.detail).toBe("Focus");
    vi.advanceTimersByTime(3_600_000);
    expect(session().phase).toBe("focus");
    timer.startNext();
    expect(calls.alarm).toEqual([true, false]);
    expect(session().phase).toBe("shortBreak");
    expect(session().duration).toBe(120);
  });

  it("ends the session when the section is switched off", () => {
    timer.start("timer", 5);
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, sections: { ...DEFAULT_SETTINGS.sections, timer: false } } });
    timer.syncWithPreferences();
    expect(model.hasSession(session())).toBe(false);
    timer.start("timer", 5);
    expect(model.hasSession(session())).toBe(false);
  });

  it("silences a ringing timer when sound is turned off", () => {
    timer.start("timer", 1);
    vi.advanceTimersByTime(61_000);
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, timerSoundEnabled: false } });
    timer.syncWithPreferences();
    expect(calls.alarm).toEqual([true, false]);
    expect(session().completed).toBe(true);
  });
});
