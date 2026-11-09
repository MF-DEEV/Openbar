// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// A sound when a session starts waiting for the person (#167): once for each
// permission, question or plan, and never for a turn that just ended.

import { hidesAutoApproval } from "./presentation";
import { isWaiting, type Session } from "./types";

export interface WaitingRequest {
  session: string;
  /** Changes with each new request of the session. */
  request: string;
}

/** What the sessions wait on now; Codex requests it answers by itself are left out. */
export function waitingRequests(sessions: readonly Session[], autoAllow: Record<string, boolean>): WaitingRequest[] {
  return sessions
    .filter((session) => isWaiting(session.phase) && !hidesAutoApproval(session, autoAllow))
    .map((session) => ({
      session: session.id,
      request: session.permission?.requestId ?? `${session.id}:${session.phase}:${session.phaseSince}`,
    }));
}

/**
 * Tells which requests are new since the last snapshot. The first snapshot
 * only sets what is known, so requests already waiting when the app starts
 * stay quiet.
 */
export class WaitingSound {
  private known: Set<string> | null = null;

  arrived(requests: readonly WaitingRequest[]): WaitingRequest[] {
    const before = this.known;
    this.known = new Set(requests.map((r) => r.request));
    if (!before) return [];
    return requests.filter((r) => !before.has(r.request));
  }
}

/** Whether the person is already looking at the session's page, or the list holding it. */
export function inView(
  island: { expanded: boolean; selected: string },
  focused: string | null,
  session: string,
): boolean {
  return island.expanded && island.selected === "sessions" && (focused === null || focused === session);
}
