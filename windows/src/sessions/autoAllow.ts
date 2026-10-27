// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { SETTLE_DELAY, useSessions } from "./store";
import type { Session } from "./types";

export function automaticallyAllows(session: Session): boolean {
  return (
    session.provider === "codex" &&
    !!useSessions.getState().autoAllow[session.id] &&
    session.phase === "waitingPermission" &&
    !!session.permission &&
    !session.permission.questions.length
  );
}

/** Runs independently of the open page; rechecks each request before sending. */
export function subscribeAutoAllow(): () => void {
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const attempted = new Set<string>();
  let disposed = false;
  const sync = () => {
    const state = useSessions.getState();
    const pending = new Map(
      state.snapshot.sessions
        .filter(automaticallyAllows)
        .map((s) => [JSON.stringify([s.id, s.permission!.requestId]), s]),
    );
    for (const [key, timer] of timers)
      if (!pending.has(key)) {
        clearTimeout(timer);
        timers.delete(key);
      }
    const live = new Set(
      state.snapshot.sessions.filter((s) => s.permission).map((s) => JSON.stringify([s.id, s.permission!.requestId])),
    );
    for (const key of attempted) if (!live.has(key)) attempted.delete(key);
    for (const [key, session] of pending) {
      if (timers.has(key) || attempted.has(key)) continue;
      const since = state.shownAt[session.permission!.requestId];
      if (since === undefined) continue;
      timers.set(
        key,
        setTimeout(
          () => {
            timers.delete(key);
            const current = useSessions.getState().snapshot.sessions.find((s) => s.id === session.id);
            if (
              disposed ||
              !current ||
              !automaticallyAllows(current) ||
              current.permission!.requestId !== session.permission!.requestId
            )
              return;
            attempted.add(key);
            void useSessions
              .getState()
              .decide(current, "allow")
              .then((ok) => {
                if (!ok) throw new Error("Approval was not accepted");
              })
              .catch(() => {
                if (!disposed) useSessions.setState((s) => ({ autoAllow: { ...s.autoAllow, [session.id]: false } }));
              });
          },
          Math.max(0, (since + SETTLE_DELAY) * 1000 - Date.now()) + 10,
        ),
      );
    }
  };
  const unsubscribe = useSessions.subscribe(sync);
  sync();
  return () => {
    disposed = true;
    unsubscribe();
    for (const timer of timers.values()) clearTimeout(timer);
    timers.clear();
  };
}
