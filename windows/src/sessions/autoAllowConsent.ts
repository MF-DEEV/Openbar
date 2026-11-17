// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Session } from "./types";

const KEY = "sessions.autoAllowTurns";
/** A consent this old belongs to a turn that is long over. */
const MAX_AGE = 24 * 60 * 60;
interface Consent {
  cwd: string;
  turnStarted: number;
}

// Codex sessions have no pid (one thread can move between terminals); the
// thread id with Codex's own start of the turn names the task.
const working = (session: Session) =>
  session.provider === "codex" && session.phase !== "finished" && session.phase !== "idle";

/** Restore only the exact task the user enabled, never a later turn or another thread. */
export function autoAllowConsent(now = Date.now() / 1000) {
  const pending: Record<string, Consent> = {};
  try {
    const saved: unknown = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "{}");
    if (saved && typeof saved === "object" && !Array.isArray(saved)) {
      for (const [id, value] of Object.entries(saved)) {
        const consent = value as Partial<Consent> | null;
        if (
          consent &&
          typeof consent === "object" &&
          typeof consent.cwd === "string" &&
          typeof consent.turnStarted === "number" &&
          Number.isFinite(consent.turnStarted) &&
          now - consent.turnStarted < MAX_AGE
        )
          pending[id] = { cwd: consent.cwd, turnStarted: consent.turnStarted };
      }
    }
  } catch {
    /* Storage is optional. In-memory Autoaccept still works. */
  }

  return {
    restore(sessions: Session[]): Record<string, boolean> {
      const restored: Record<string, boolean> = {};
      for (const session of sessions) {
        const saved = pending[session.id];
        if (!saved) continue;
        if (!working(session)) {
          // The turn ended while the app was closed; its consent ended with it.
          delete pending[session.id];
        } else if (session.cwd === saved.cwd && session.turnStarted === saved.turnStarted) {
          restored[session.id] = true;
          delete pending[session.id];
        }
        // Otherwise the turn's exact start is not known yet (Codex reports it
        // just after the thread is joined), or this is another turn. Keep the
        // grant pending, but never activate it until it matches.
      }
      return restored;
    },
    save(sessions: Session[], enabled: Record<string, boolean>) {
      const saved = { ...pending };
      for (const session of sessions) {
        // An undecided grant stays until restore settles it or a new choice replaces it.
        if (pending[session.id] && !enabled[session.id]) continue;
        delete pending[session.id];
        delete saved[session.id];
        if (
          enabled[session.id] &&
          working(session) &&
          session.turnStarted !== undefined &&
          Number.isFinite(session.turnStarted)
        ) {
          saved[session.id] = { cwd: session.cwd, turnStarted: session.turnStarted };
        }
      }
      try {
        globalThis.localStorage?.setItem(KEY, JSON.stringify(saved));
      } catch {
        /* Storage is optional. */
      }
    },
  };
}
