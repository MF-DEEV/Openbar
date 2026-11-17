// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The running timer (NotchTimerService.swift). The session lives in memory
// only: a relaunch never resurrects a timer from another day. While it runs,
// a tick lands just after each whole second of its reading; Rust's wake-up
// at the deadline covers a webview that throttles its timers while hidden.

import { Hourglass } from "lucide-react";
import { create } from "zustand";
import { modulesOf, useIsland } from "../island/store";
import { endMascotCountdown, reactMascot, watchMascotCountdown } from "../mascot/service";
import { timerReaction } from "../mascot/model";
import { COUNTDOWN_LEAD } from "../mascot/motion";
import { bridge } from "../shared/bridge";
import { clock } from "./clock";
import { timerPref } from "./prefs";
import * as model from "./model";
import { phaseTitle, T } from "./strings";

/** System orange (dark appearance). */
export const ORANGE = "#ff9f0a";

export interface TimerState {
  session: model.TimerSession;
  /** The clock as of the last tick; the readings render from it. */
  now: number;
}

export const useTimer = create<TimerState>(() => ({ session: model.IDLE, now: clock.now() }));

/** The island is on and the Timer section is shown (NotchTimerSupport.isEnabled). */
export function timerEnabled(): boolean {
  const island = useIsland.getState();
  return island.settings.enabled && modulesOf(island).includes("timer");
}

/** The Pomodoro values as the page shows them, a pending edit included. */
export function pomodoroConfig(): model.PomodoroConfig {
  return model.pomodoroConfig({
    focusMinutes: timerPref("pomodoroFocusMinutes"),
    shortBreakMinutes: timerPref("pomodoroShortBreakMinutes"),
    longBreakMinutes: timerPref("pomodoroLongBreakMinutes"),
    longBreakInterval: timerPref("pomodoroLongBreakInterval"),
    totalSessions: timerPref("pomodoroTotalSessions"),
  });
}

/** The title a session reads by: its phase, or how it ended. */
export function sessionTitle(s: model.TimerSession): string {
  if (model.cycleFinished(s)) return T.pomodoroFinished;
  return s.completed ? T.finished : phaseTitle(s.phase);
}

let ringing = false;
function alarm(on: boolean) {
  if (ringing === on) return;
  ringing = on;
  void bridge.sound.alarm("timer", on);
}

let tickTimer: ReturnType<typeof setTimeout> | undefined;
let wakeDeadline: number | null = null;

/** The next tick just past the reading's next whole second, and Rust's wake-up at the deadline. */
function schedule() {
  clearTimeout(tickTimer);
  tickTimer = undefined;
  const { session } = useTimer.getState();
  if (model.isRunning(session)) {
    const now = clock.now();
    tickTimer = setTimeout(tick, model.secondBoundaryOffset(session, now) * 1000 + 15);
  }
  const end = model.deadline(session);
  if (end !== wakeDeadline) {
    wakeDeadline = end;
    void bridge.timer.wake(end === null ? null : Math.max(0, (end - clock.now()) * 1000));
  }
}

function commit(session: model.TimerSession) {
  useTimer.setState({ session, now: clock.now() });
  schedule();
  scheduleMascotCountdown();
}

let countdownTimer: ReturnType<typeof setTimeout> | undefined;

/** The companion comes out for a countdown's last seconds; paused, it goes back behind the camera. */
function scheduleMascotCountdown() {
  clearTimeout(countdownTimer);
  const end = model.deadline(useTimer.getState().session);
  if (end === null) {
    endMascotCountdown(true);
    return;
  }
  const remaining = end - clock.now();
  if (remaining <= 1.5) return;
  countdownTimer = setTimeout(
    () => {
      const at = model.deadline(useTimer.getState().session);
      if (at !== null) watchMascotCountdown(at - clock.now());
    },
    Math.max(0, remaining - COUNTDOWN_LEAD) * 1000,
  );
}

/** Completes a countdown whose deadline passed: the notice, and the chime when sound is on. */
function finishIfDue(): boolean {
  const done = model.finishIfDue(useTimer.getState().session, clock.now());
  if (!done) return false;
  endMascotCountdown(false);
  commit(done);
  const reaction = timerReaction(done.phase);
  useIsland.getState().showNotice({
    id: `timer:${Date.now()}`,
    kind: "timer",
    title: model.cycleFinished(done) ? T.pomodoroFinished : T.finished,
    detail: phaseTitle(done.phase),
    priority: 2,
    duration: 6,
    opens: "timer",
    icon: Hourglass,
    tint: ORANGE,
    mascot: reaction,
  });
  reactMascot(reaction);
  if (useIsland.getState().settings.timerSoundEnabled) alarm(true);
  return true;
}

/** Moves the clock on. After a jump the clock wasn't told about (a sleep), it measures again first. */
function tick() {
  if (clock.jumped()) {
    void clock.resync().then(tick);
    return;
  }
  if (!finishIfDue()) commit(useTimer.getState().session);
}

/** Measures the clock again, then catches up: on return from sleep, from hiding, or at Rust's wake-up. */
export function catchUp() {
  void clock.resync().then(() => {
    if (model.hasSession(useTimer.getState().session)) tick();
  });
}

export const timer = {
  start(mode: model.TimerMode, minutes: number) {
    const { session } = useTimer.getState();
    if (!timerEnabled() || model.hasSession(session)) return;
    alarm(false);
    commit(model.start(session, mode, minutes, clock.now(), pomodoroConfig()));
    reactMascot("ready");
  },

  pauseOrResume() {
    if (!timerEnabled()) return;
    if (finishIfDue()) return;
    const { session } = useTimer.getState();
    const now = clock.now();
    if (model.isRunning(session)) commit(model.pause(session, now));
    else if (model.isPaused(session)) commit(model.resume(session, now));
  },

  startNext() {
    const { session } = useTimer.getState();
    if (!timerEnabled() || !model.canStartNext(session)) return;
    alarm(false);
    commit(model.startNext(session, clock.now()));
    reactMascot("ready");
  },

  /** Cancel, or Done once completed: the chime stops with it. */
  cancel() {
    alarm(false);
    endMascotCountdown(false);
    commit(model.IDLE);
  },

  /** Preferences changed: a section switched off ends the session; sound turned off silences it. */
  syncWithPreferences() {
    if (!timerEnabled()) {
      if (model.hasSession(useTimer.getState().session) || ringing) timer.cancel();
      return;
    }
    if (!useIsland.getState().settings.timerSoundEnabled) alarm(false);
  },
};
