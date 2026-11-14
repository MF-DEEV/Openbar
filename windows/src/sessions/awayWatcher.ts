// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// When the person steps away and comes back (#215). Away begins with the
// lock screen or after the set minutes without input; it ends with the
// unlock or the first input after. The sessions as they stood when the
// person was last at the computer are kept, so the card on return tells
// what changed since, the minutes it took to count as away included.

import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import { awayLines, awayMark, type AwayMark, withNewSessions } from "./away";
import { presentAway } from "./progress/AwayCard";
import { useSessions } from "./store";

/** How often input idleness is read; more often while away, to catch the return. */
const POLL = 20_000;
const POLL_AWAY = 3_000;
/** Input this recent counts as the person being here. */
const HERE = 30;

/** Exposed for the browser demo and tests. */
export const awayState = {
  /** The sessions when the person was last seen working. */
  seen: null as AwayMark | null,
  /** Set while the person is away: the mark the card will compare with. */
  away: null as AwayMark | null,
  /** The session is locked: only the unlock ends this time away, never an
   *  input reading, which still holds the last key pressed before the lock. */
  locked: false,
};

const now = () => Date.now() / 1000;
const mark = () => awayMark(useSessions.getState().snapshot.sessions, now());

/** The person stepped away; the last mark from while they were here stands. */
export function wentAway() {
  if (awayState.away) return;
  awayState.away = awayState.seen ?? mark();
}

/** The person is back: the card, when anything changed and it is wanted. */
export function cameBack(delay = 0) {
  const since = awayState.away;
  awayState.away = null;
  awayState.seen = mark();
  if (!since || !useSessions.getState().prefs.awayCard) return;
  // After an unlock, a moment for the desktop to come back before the island opens.
  setTimeout(() => {
    const lines = awayLines(since, useSessions.getState().snapshot.sessions);
    if (lines.length > 0) presentAway(lines, since.at);
  }, delay);
}

async function poll() {
  const idle = awayState.locked ? null : await bridge.session.idleSeconds().catch(() => null);
  // A reading begun before a lock that came meanwhile says nothing of now.
  if (typeof idle === "number" && !awayState.locked) {
    const limit = Math.max(1, useSessions.getState().prefs.awayMinutes) * 60;
    if (awayState.away && idle < HERE) cameBack();
    else if (!awayState.away && idle >= limit) wentAway();
    else if (!awayState.away && idle < HERE) awayState.seen = mark();
  }
  setTimeout(() => void poll(), awayState.away ? POLL_AWAY : POLL);
}

let started = false;

export function startAwayWatcher() {
  if (started) return;
  started = true;
  void bridge.session.onLock(() => {
    awayState.locked = true;
    wentAway();
  });
  void bridge.session.onUnlock(() => {
    awayState.locked = false;
    cameBack(800);
  });
  // The card goes with the island: closed, or opened on something else.
  useIsland.subscribe((s, prev) => {
    if (prev.expanded && !s.expanded && useSessions.getState().away) useSessions.setState({ away: null });
  });
  // A session that shows up while the person is here joins the mark as it
  // stands, the first snapshot after launch included.
  useSessions.subscribe((s, prev) => {
    if (s.snapshot !== prev.snapshot && !awayState.away && awayState.seen) {
      awayState.seen = withNewSessions(awayState.seen, s.snapshot.sessions);
    }
  });
  awayState.seen = mark();
  void poll();
}
