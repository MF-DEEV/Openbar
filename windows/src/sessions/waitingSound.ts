// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// A sound when a session starts waiting for the person (#167): once for each
// permission, question or plan, and never for a turn that just ended. It
// stays quiet when the island shows the session, or when its own terminal
// tab is in front (#210).

import { hidesAutoApproval } from "./presentation";
import { isWaiting, type Session } from "./types";

export interface WaitingRequest {
  session: string;
  /** The wait the request belongs to: its phase and when it began. */
  wait: string;
  /** The hook's request, once it has asked. */
  id: string | null;
}

/** What the sessions wait on now; Codex requests it answers by itself are left out. */
export function waitingRequests(sessions: readonly Session[], autoAllow: Record<string, boolean>): WaitingRequest[] {
  return sessions
    .filter((session) => isWaiting(session.phase) && !hidesAutoApproval(session, autoAllow))
    .map((session) => ({
      session: session.id,
      wait: `${session.phase}:${session.phaseSince}`,
      id: session.permission?.requestId ?? null,
    }));
}

/**
 * One prompt as two readings: the terminal's record and the hook can tell of
 * it a moment apart, the hook naming it and the record not. So one wait with
 * no name on either side is the same prompt; two names differ only when both
 * are known (the Mac's NotchSessionAttention.Request.isSame).
 */
export function samePrompt(a: WaitingRequest, b: WaitingRequest): boolean {
  if (a.wait !== b.wait) return a.id !== null && a.id === b.id;
  return a.id === null || b.id === null || a.id === b.id;
}

/**
 * Tells which requests are new since the last snapshot. The first snapshot
 * only sets what is known, so requests already waiting when the app starts
 * stay quiet.
 */
export class WaitingSound {
  private known: Map<string, WaitingRequest> | null = null;

  arrived(requests: readonly WaitingRequest[]): WaitingRequest[] {
    const before = this.known;
    const next = new Map<string, WaitingRequest>();
    const fresh: WaitingRequest[] = [];
    for (const request of requests) {
      const old = before?.get(request.session);
      if (old && samePrompt(old, request)) {
        // The name, once known, tells the next prompt of the same wait apart.
        next.set(request.session, old.id === null ? request : old);
      } else {
        next.set(request.session, request);
        if (before) fresh.push(request);
      }
    }
    this.known = next;
    return fresh;
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

/** How long the terminals are asked before the sound plays anyway. */
export const TERMINAL_CHECK_LIMIT_MS = 1500;

/**
 * Whether to play for the requests the island doesn't show (#210): quiet only
 * when their terminals say every one of them is in front in its own tab or
 * pane. A terminal that can't tell, fails or is slow to answer plays, as the
 * Mac's NotchSessionWaitingSound.shouldPlay.
 */
export async function shouldPlay(
  unseen: readonly string[],
  terminalShown: (sessionIds: string[]) => Promise<boolean | undefined>,
  limitMs = TERMINAL_CHECK_LIMIT_MS,
): Promise<boolean> {
  if (unseen.length === 0) return false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<false>((resolve) => {
    timer = setTimeout(() => resolve(false), limitMs);
  });
  try {
    return !(await Promise.race([terminalShown([...unseen]), late]));
  } catch {
    return true;
  } finally {
    clearTimeout(timer);
  }
}
