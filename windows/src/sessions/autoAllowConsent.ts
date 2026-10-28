// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Session } from "./types";

const KEY = "sessions.autoAllowTurns";
interface Consent {
  pid: number;
  cwd: string;
  turnStarted: number;
}

/** Restore only the exact task the user enabled, never a later turn or another process. */
export function autoAllowConsent() {
  let pending: Record<string, Consent> = {};
  try {
    const saved: unknown = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "{}");
    if (saved && typeof saved === "object" && !Array.isArray(saved)) {
      for (const [id, value] of Object.entries(saved)) {
        const consent = value as Partial<Consent> | null;
        if (
          consent &&
          typeof consent === "object" &&
          typeof consent.pid === "number" &&
          Number.isInteger(consent.pid) &&
          consent.pid > 0 &&
          typeof consent.cwd === "string" &&
          typeof consent.turnStarted === "number" &&
          Number.isFinite(consent.turnStarted)
        )
          pending[id] = consent as Consent;
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
        // Discovery may initially publish an empty snapshot. Keep unseen grants
        // pending, but never activate them until their full identity is known.
        if (
          session.provider === "codex" &&
          session.phase !== "finished" &&
          session.phase !== "idle" &&
          session.pid === saved.pid &&
          session.cwd === saved.cwd &&
          session.turnStarted === saved.turnStarted
        ) {
          restored[session.id] = true;
        }
        delete pending[session.id];
      }
      return restored;
    },
    save(sessions: Session[], enabled: Record<string, boolean>) {
      const saved = { ...pending };
      for (const session of sessions) {
        delete saved[session.id];
        if (
          enabled[session.id] &&
          session.provider === "codex" &&
          session.phase !== "finished" &&
          session.phase !== "idle" &&
          session.pid > 0 &&
          session.turnStarted !== undefined &&
          Number.isFinite(session.turnStarted)
        ) {
          saved[session.id] = { pid: session.pid, cwd: session.cwd, turnStarted: session.turnStarted };
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
