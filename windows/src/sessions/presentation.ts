// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Session, Snapshot } from "./types";

type AutoAllow = Record<string, boolean>;

/** Include the brief status-only frames before/after the actual request. */
export function hidesAutoApproval(session: Session, autoAllow: AutoAllow): boolean {
  return (
    !!autoAllow[session.id] &&
    session.provider === "codex" &&
    session.phase === "waitingPermission" &&
    session.waitingFor !== "terminal" &&
    !session.permission?.questions.length
  );
}

// Selectors must return stable references until their inputs change.
const cache = new WeakMap<Snapshot, WeakMap<AutoAllow, Snapshot>>();

/** Keep authoritative requests intact; present automatic approvals as blue work. */
export function presentedSnapshot({ snapshot, autoAllow }: { snapshot: Snapshot; autoAllow: AutoAllow }): Snapshot {
  let byMode = cache.get(snapshot);
  const cached = byMode?.get(autoAllow);
  if (cached) return cached;
  let hidden = 0;
  const sessions = snapshot.sessions.map((session) => {
    if (!hidesAutoApproval(session, autoAllow)) return session;
    hidden++;
    return {
      ...session,
      phase: "generating" as const,
      phaseSince: session.turnStarted ?? session.phaseSince,
      permission: undefined,
      waitingFor: undefined,
    };
  });
  const result = hidden
    ? {
        ...snapshot,
        sessions,
        counts: {
          ...snapshot.counts,
          waiting: Math.max(0, snapshot.counts.waiting - hidden),
          working: snapshot.counts.working + hidden,
          workingCodex: snapshot.counts.workingCodex + hidden,
        },
      }
    : snapshot;
  if (!byMode) {
    byMode = new WeakMap();
    cache.set(snapshot, byMode);
  }
  byMode.set(autoAllow, result);
  return result;
}
