// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Controls page's timer tile reads the session here: while one exists it
// shows the live clock and the phase in orange (NotchControlsView's timer
// shortcut).

import { Timer, type LucideIcon } from "lucide-react";
import * as model from "./model";
import { sessionTitle, useTimer } from "./store";
import { T } from "./strings";
import { markIcon } from "./TimerStrip";

export interface TimerTile {
  /** A session exists: draw the tile in orange with its reading. */
  active: boolean;
  /** The phase ("Focus", "Time is up"), or "Timer" with no session. */
  title: string;
  /** The page's clock, `MM:SS` or `H:MM:SS`, ticking each second while it runs. */
  reading?: string;
  icon: LucideIcon;
}

export function useTimerTile(): TimerTile {
  const session = useTimer((s) => s.session);
  const now = useTimer((s) => s.now);
  if (!model.hasSession(session)) return { active: false, title: T.timer, icon: Timer };
  return {
    active: true,
    title: sessionTitle(session),
    reading: model.clockTextFor(session, now),
    icon: markIcon(session),
  };
}
