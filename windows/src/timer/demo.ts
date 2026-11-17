// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample sessions for designing in a plain browser:
// island.html?timer (a 15-minute timer), ?timer=paused, ?timer=stopwatch,
// ?timer=pomodoro or ?timer=done (a focus that just ended).

import { clock } from "./clock";
import * as model from "./model";
import { timer, useTimer } from "./store";

export function startDemo(kind: string) {
  const now = clock.now();
  switch (kind) {
    case "stopwatch":
      timer.start("stopwatch", 0);
      useTimer.setState({ session: { ...useTimer.getState().session, anchor: now - 754 } });
      return;
    case "pomodoro":
      timer.start("pomodoro", 25);
      return;
    case "paused":
      timer.start("timer", 15);
      timer.pauseOrResume();
      return;
    case "done":
      useTimer.setState({ session: model.start(model.IDLE, "pomodoro", 25, now - 25 * 60) });
      timer.pauseOrResume();
      return;
    default:
      timer.start("timer", 15);
  }
}
