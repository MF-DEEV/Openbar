// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Timer section's English strings, as the Mac words them
// (NotchActivityStrings.swift, Localization.swift).

import type { PomodoroOption, TimerMode, TimerPhase } from "./model";

export const T = {
  timer: "Timer",
  pomodoro: "Pomodoro",
  stopwatch: "Stopwatch",
  focus: "Focus",
  shortBreak: "Short break",
  longBreak: "Long break",
  pomodoroHint: "Start each phase when you are ready. The cycle ends after the last focus session.",
  totalSessions: "Total sessions",
  longBreakInterval: "Sessions between long breaks",
  sessionProgress: (n: number, of: number) => `Session ${n} of ${of}`,
  pomodoroFinished: "Pomodoro complete",
  start: "Start",
  resume: "Resume",
  pause: "Pause",
  finished: "Time is up",
  cancel: "Cancel",
  done: "Done",
};

export const modeTitle = (mode: TimerMode) => T[mode];

export const phaseTitle = (phase: TimerPhase) => T[phase];

export const optionTitle = (option: PomodoroOption) =>
  ({
    focusMinutes: T.focus,
    shortBreakMinutes: T.shortBreak,
    longBreakMinutes: T.longBreak,
    longBreakInterval: T.longBreakInterval,
    totalSessions: T.totalSessions,
  })[option];
