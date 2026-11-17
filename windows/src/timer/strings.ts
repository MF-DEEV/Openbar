// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Timer section's English strings, as the Mac words them
// (NotchActivityStrings.swift, Localization.swift).

import { t, tf } from "../i18n";

import type { PomodoroOption, TimerMode, TimerPhase } from "./model";

export const T = {
  get timer() {
    return t("notchActivities.timer");
  },
  get pomodoro() {
    return t("notchActivities.pomodoro");
  },
  get stopwatch() {
    return t("notchActivities.stopwatch");
  },
  get focus() {
    return t("notchActivities.focus");
  },
  get shortBreak() {
    return t("notchActivities.shortBreak");
  },
  get longBreak() {
    return t("notchActivities.longBreak");
  },
  get pomodoroHint() {
    return t("notchActivities.pomodoroHint");
  },
  get totalSessions() {
    return t("notchActivities.totalSessions");
  },
  get longBreakInterval() {
    return t("notchActivities.longBreakInterval");
  },
  sessionProgress: (n: number, of: number) => tf("notchActivities.sessionProgress", n, of),
  get pomodoroFinished() {
    return t("notchActivities.pomodoroFinished");
  },
  get start() {
    return t("notchActivities.start");
  },
  get resume() {
    return t("notchActivities.resume");
  },
  get pause() {
    return t("s.actionPause");
  },
  get finished() {
    return t("notchActivities.finished");
  },
  get cancel() {
    return t("clipboard.cancel");
  },
  get done() {
    return t("feedback.done");
  },
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
