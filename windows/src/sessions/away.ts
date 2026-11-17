// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// "While you were away" (#215): how the sessions stood when the person was
// last at the computer, and what changed by the time they came back. Built
// from what the island already knows; no AI service is asked anything.

import { isWaiting, isWorking, type Session } from "./types";

/** One session as it stood. */
interface Before {
  phase: Session["phase"];
  done: number;
  added: number;
  removed: number;
}

/** The sessions when the person was last seen. */
export interface AwayMark {
  at: number;
  sessions: Record<string, Before>;
}

export type Outcome = "waiting" | "error" | "limited" | "finished" | "working";

/** One tile of the card: a session that changed, by id. */
export interface AwayLine {
  id: string;
  outcome: Outcome;
  files: number;
  added: number;
  removed: number;
  /** Tasks ticked off while away, and where the list stands. */
  tasks?: { from: number; to: number; total: number };
}

export function awayMark(sessions: Session[], at: number): AwayMark {
  return {
    at,
    sessions: Object.fromEntries(
      sessions.map((s) => [
        s.id,
        { phase: s.phase, done: s.tasks?.done ?? 0, added: s.edits?.added ?? 0, removed: s.edits?.removed ?? 0 },
      ]),
    ),
  };
}

/** The mark with the sessions it has not met yet added as they stand now:
 *  ones that showed up while the person was here, so the card counts only
 *  what they did after. */
export function withNewSessions(mark: AwayMark, sessions: Session[]): AwayMark {
  const fresh = sessions.filter((s) => !(s.id in mark.sessions));
  if (fresh.length === 0) return mark;
  return { at: mark.at, sessions: { ...mark.sessions, ...awayMark(fresh, mark.at).sessions } };
}

const ORDER: Outcome[] = ["waiting", "error", "limited", "finished", "working"];

/** What each session did since the mark; a session that did nothing has no line. */
export function awayLines(mark: AwayMark, sessions: Session[]): AwayLine[] {
  const lines: AwayLine[] = [];
  for (const s of sessions) {
    const before = mark.sessions[s.id];
    const outcome = outcomeOf(s, before, mark.at);
    if (!outcome) continue;
    const files = (s.edits?.touched ?? []).filter((t) => t > mark.at).length;
    const line: AwayLine = {
      id: s.id,
      outcome,
      files,
      added: files ? Math.max(0, (s.edits?.added ?? 0) - (before?.added ?? 0)) : 0,
      removed: files ? Math.max(0, (s.edits?.removed ?? 0) - (before?.removed ?? 0)) : 0,
    };
    if (s.tasks && s.tasks.total > 0) {
      const from = Math.min(before?.done ?? 0, s.tasks.done);
      if (s.tasks.done > from) line.tasks = { from, to: s.tasks.done, total: s.tasks.total };
    }
    // A session still at work shows only when it moved on.
    if (outcome === "working" && !line.files && !line.tasks) continue;
    lines.push(line);
  }
  return lines.sort((a, b) => ORDER.indexOf(a.outcome) - ORDER.indexOf(b.outcome));
}

function outcomeOf(s: Session, before: Before | undefined, since: number): Outcome | null {
  const changed = s.phaseSince > since || !before || before.phase !== s.phase;
  if (isWaiting(s.phase)) return changed ? "waiting" : null;
  if (s.phase === "limited") return changed ? "limited" : null;
  if ((s.failedAt ?? 0) > since && !isWorking(s.phase)) return "error";
  if (s.phase === "finished" || s.phase === "idle") {
    const wasWorking = before ? isWorking(before.phase) || isWaiting(before.phase) : (s.turnStarted ?? 0) > since;
    return wasWorking && (s.lastActive ?? s.phaseSince) > since ? "finished" : null;
  }
  if (isWorking(s.phase)) return "working";
  return null;
}
