// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { tf } from "../i18n/index";

// The timer's model (NotchTimerSupport.swift; spec 04 §4.1): a session is a
// value, every change returns a new one, and nothing reads a clock of its
// own. The anchor is a time on the timer's continuous clock: a countdown's
// deadline, or the instant a stopwatch read zero. Refreshes never subtract
// ticks, so sleep and busy frames cannot drift.

export type TimerMode = "timer" | "pomodoro" | "stopwatch";
export type TimerPhase = "timer" | "focus" | "shortBreak" | "longBreak" | "stopwatch";
export const TIMER_MODES: readonly TimerMode[] = ["timer", "pomodoro", "stopwatch"];

/** Every Pomodoro value the Mac accepts (NotchPomodoroConfiguration). */
export const POMODORO_RANGES = {
  focusMinutes: [1, 180],
  shortBreakMinutes: [1, 60],
  longBreakMinutes: [1, 60],
  longBreakInterval: [1, 24],
  totalSessions: [1, 24],
} as const;
export type PomodoroOption = keyof typeof POMODORO_RANGES;

/** Preferences describe the next cycle; a running cycle owns an immutable copy. */
export interface PomodoroConfig {
  focusMinutes: number;
  shortBreakMinutes: number;
  longBreakMinutes: number;
  longBreakInterval: number;
  totalSessions: number;
}

export const DEFAULT_POMODORO: PomodoroConfig = {
  focusMinutes: 25,
  shortBreakMinutes: 5,
  longBreakMinutes: 15,
  longBreakInterval: 4,
  totalSessions: 4,
};

const clampInt = (value: number, low: number, high: number, fallback: number) =>
  Number.isFinite(value) ? Math.min(high, Math.max(low, Math.round(value))) : fallback;

export const clampOption = (option: PomodoroOption, value: number) =>
  clampInt(value, POMODORO_RANGES[option][0], POMODORO_RANGES[option][1], DEFAULT_POMODORO[option]);

export function pomodoroConfig(values: Partial<PomodoroConfig> = {}): PomodoroConfig {
  const out = { ...DEFAULT_POMODORO };
  for (const option of Object.keys(POMODORO_RANGES) as PomodoroOption[]) {
    out[option] = clampOption(option, values[option] ?? DEFAULT_POMODORO[option]);
  }
  return out;
}

export const TIMER_LIMIT = 180 * 60;
/** A stopwatch keeps counting; its clock saturates at two hour digits. */
export const STOPWATCH_LIMIT = 100 * 3600 - 1;

export const timerMinutes = (value: number) => clampInt(value, 1, 180, 15);

export interface TimerSession {
  mode: TimerMode;
  phase: TimerPhase;
  /** Seconds. */
  duration: number;
  anchor: number | null;
  pausedReading: number | null;
  completed: boolean;
  completedFocuses: number;
  config: PomodoroConfig;
}

export const IDLE: TimerSession = {
  mode: "timer",
  phase: "timer",
  duration: 300,
  anchor: null,
  pausedReading: null,
  completed: false,
  completedFocuses: 0,
  config: DEFAULT_POMODORO,
};

export const countsUp = (s: TimerSession) => s.mode === "stopwatch";
export const isRunning = (s: TimerSession) => s.anchor !== null;
export const isPaused = (s: TimerSession) => s.pausedReading !== null;
export const hasSession = (s: TimerSession) => isRunning(s) || isPaused(s) || s.completed;
export const deadline = (s: TimerSession) => (countsUp(s) ? null : s.anchor);
export const cycleFinished = (s: TimerSession) =>
  s.mode === "pomodoro" && s.completed && s.completedFocuses >= s.config.totalSessions;
export const canStartNext = (s: TimerSession) => s.mode === "pomodoro" && s.completed && !cycleFinished(s);
export const sessionNumber = (s: TimerSession) =>
  Math.min(s.config.totalSessions, s.completedFocuses + (s.phase === "focus" && !s.completed ? 1 : 0));

/** Seconds left in a countdown, or elapsed on a stopwatch. */
export function reading(s: TimerSession, now: number): number {
  if (s.anchor !== null) return Math.max(0, countsUp(s) ? now - s.anchor : s.anchor - now);
  return s.pausedReading ?? (s.completed || countsUp(s) ? 0 : s.duration);
}

export function start(
  s: TimerSession,
  mode: TimerMode,
  minutes: number,
  now: number,
  config: PomodoroConfig = DEFAULT_POMODORO,
): TimerSession {
  if (hasSession(s) || !Number.isFinite(now)) return s;
  const base = { ...IDLE, mode, config };
  switch (mode) {
    case "timer": {
      const duration = timerMinutes(minutes) * 60;
      return { ...base, phase: "timer", duration, anchor: now + duration };
    }
    case "pomodoro": {
      const duration = config.focusMinutes * 60;
      return { ...base, phase: "focus", duration, anchor: now + duration };
    }
    case "stopwatch":
      return { ...base, phase: "stopwatch", duration: 0, anchor: now };
  }
}

/** The completed session, or null while the countdown has time left. */
export function finishIfDue(s: TimerSession, now: number): TimerSession | null {
  const end = deadline(s);
  if (end === null || !Number.isFinite(now) || now < end) return null;
  return {
    ...s,
    anchor: null,
    pausedReading: null,
    completed: true,
    completedFocuses: s.completedFocuses + (s.phase === "focus" ? 1 : 0),
  };
}

/** Pause stores the reading; a countdown already due completes instead. */
export function pause(s: TimerSession, now: number): TimerSession {
  if (!isRunning(s) || !Number.isFinite(now)) return s;
  const finished = finishIfDue(s, now);
  if (finished) return finished;
  return { ...s, pausedReading: reading(s, now), anchor: null };
}

/** Resume re-anchors the stored reading on the clock. */
export function resume(s: TimerSession, now: number): TimerSession {
  if (s.pausedReading === null || !Number.isFinite(now)) return s;
  const r = s.pausedReading;
  return { ...s, pausedReading: null, anchor: countsUp(s) ? now - r : now + r };
}

/** After a focus, the long break every `longBreakInterval` focuses, else the short one; after a break, focus. */
export function nextPhase(s: TimerSession): TimerPhase {
  if (s.phase !== "focus") return "focus";
  return s.completedFocuses % s.config.longBreakInterval === 0 ? "longBreak" : "shortBreak";
}

/**
 * A new phase always starts by an explicit action, so returning from a long
 * sleep cannot silently complete work and break cycles never taken.
 */
export function startNext(s: TimerSession, now: number): TimerSession {
  if (!canStartNext(s) || !Number.isFinite(now)) return s;
  const phase = nextPhase(s);
  const minutes =
    phase === "focus"
      ? s.config.focusMinutes
      : phase === "longBreak"
        ? s.config.longBreakMinutes
        : s.config.shortBreakMinutes;
  const duration = minutes * 60;
  return { ...s, phase, duration, completed: false, anchor: now + duration };
}

/**
 * The closed island shows a session unless its countdown is hidden; a
 * finished one always shows until dismissed (NotchTimerSupport.showsActivity).
 */
export const showsActivity = (s: TimerSession, enabled: boolean, hideCountdown: boolean) =>
  hasSession(s) && enabled && (s.completed || !hideCountdown);

// MARK: Text

function wholeSeconds(value: number, limit: number, round: (n: number) => number): number {
  return Number.isFinite(value) ? round(Math.min(limit, Math.max(0, value))) : 0;
}

const pad = (n: number) => String(n).padStart(2, "0");

function clock(seconds: number): string {
  if (seconds >= 3600)
    return `${Math.floor(seconds / 3600)}:${pad(Math.floor(seconds / 60) % 60)}:${pad(seconds % 60)}`;
  return `${pad(Math.floor(seconds / 60))}:${pad(seconds % 60)}`;
}

/** `MM:SS`, or `H:MM:SS` from an hour; a countdown rounds up. */
export const clockText = (remaining: number) => clock(wholeSeconds(remaining, TIMER_LIMIT, Math.ceil));

/** Elapsed time rounds down: a stopwatch cannot show a second that has not passed yet. */
export const stopwatchText = (elapsed: number) => clock(wholeSeconds(elapsed, STOPWATCH_LIMIT, Math.floor));

/** "1h35", never "1:35", which beside the minute clock would pass for a minute and 35 seconds. */
export const hoursText = (hours: number, minutes: number) => `${hours}h${pad(minutes)}`;

function compactHoursText(seconds: number): string {
  return hoursText(Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60);
}

/** A narrow unit reading: `45s`, `15m`, `1h` or `1h 30m` (Duration's narrow units, seconds rounded up). */
export function durationText(remaining: number): string {
  const seconds = Number.isFinite(remaining) ? Math.ceil(Math.min(TIMER_LIMIT, Math.max(0, remaining))) : 0;
  if (seconds >= 3600) {
    const minutes = Math.floor(seconds / 60) % 60;
    return `${Math.floor(seconds / 3600)}h${minutes ? tf("windows.timer.valueM", minutes) : ""}`;
  }
  return seconds >= 60 ? `${Math.floor(seconds / 60)}m` : `${seconds}s`;
}

/** The clock a session shows on its page. */
export const clockTextFor = (s: TimerSession, now: number) =>
  countsUp(s) ? stopwatchText(reading(s, now)) : clockText(reading(s, now));

/** The closed island's short reading: `1h35`, `15m`, `45s`; a stopwatch keeps `MM:SS` until an hour. */
export function compactText(s: TimerSession, now: number): string {
  const r = reading(s, now);
  if (countsUp(s)) {
    const seconds = wholeSeconds(r, STOPWATCH_LIMIT, Math.floor);
    return seconds >= 3600 ? compactHoursText(seconds) : clock(seconds);
  }
  return r >= 3600 ? compactHoursText(wholeSeconds(r, TIMER_LIMIT, Math.floor)) : durationText(r);
}

/**
 * What a clock's digits roll on. In the closed island the seconds change in
 * place and the rest rolls: "12:04" rolls as "12" (rollingValue).
 */
export function rollingSplit(value: string): [head: string, tail: string] {
  const colon = value.lastIndexOf(":");
  return colon < 0 ? [value, ""] : [value.slice(0, colon), value.slice(colon)];
}

/** The reading's shape: the island only re-measures when a character comes or goes. */
export const readingShape = (value: string) => value.replace(/\d/g, "0");

/**
 * Seconds until the reading next crosses a whole second, so a tick lands
 * just after each change instead of drifting across a boundary.
 */
export function secondBoundaryOffset(s: TimerSession, now: number): number {
  const r = reading(s, now);
  if (!Number.isFinite(r)) return 0;
  const fraction = r % 1;
  const offset = countsUp(s) ? 1 - fraction : fraction;
  return offset > 0 ? offset : 1;
}

// MARK: Ruler

/** 14 pt per minute (NotchTimerRulerScale). */
export const RULER_SPACING = 14;

/** Moving the ruler by `points` (dragging left raises the value), within 1…180. */
export function rulerMoving(value: number, points: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(points)) return 1;
  return Math.min(180, Math.max(1, value - points / RULER_SPACING));
}

export const rulerMinute = (value: number) => Math.round(rulerMoving(value, 0));
export const rulerOffset = (minute: number, selected: number) => (minute - selected) * RULER_SPACING;
export const rulerLabel = (minute: number) =>
  minute >= 60 ? hoursText(Math.floor(minute / 60), minute % 60) : String(minute);
