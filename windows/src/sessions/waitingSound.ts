// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// A sound when a session starts waiting for the person (#167): once for each
// permission, question or plan, and never for a turn that just ended. It
// stays quiet when the island shows the session, or when its own terminal
// tab is in front (#210).

import { hidesAutoApproval } from "./presentation";
import { isWaiting, type Session } from "./types";
import { SOUND_KEYS, soundsMuted, type SessionSoundPrefs, type SoundEventKind } from "./soundPreferences";

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

export interface SessionSoundEvent {
  session: string;
  kind: SoundEventKind;
  id: string;
  at: number;
}

/** Extend the waiting detector, using agent-reported terminal facts. Tool failure
 * flags never enter this path. Snapshots contain parent rows only; their pending
 * requests already include their children (the existing session service rule). */
export class SessionSoundEvents {
  private waiting = new WaitingSound();
  private initialized = false;
  private seen = new Map<string, true>();
  constructor(private readonly since = Date.now() / 1000) {}

  arrived(sessions: readonly Session[], autoAllow: Record<string, boolean>): SessionSoundEvent[] {
    const result: SessionSoundEvent[] = [];
    const remember = (event: SessionSoundEvent) => {
      const key = JSON.stringify([event.session, event.kind, event.id]);
      if (this.seen.has(key)) return;
      this.seen.set(key, true);
      // Bound history across long app runs, including sessions removed on reconnect.
      if (this.seen.size > 4096) this.seen.delete(this.seen.keys().next().value!);
      if (this.initialized && event.at >= this.since) result.push(event);
    };
    const all = waitingRequests(sessions, autoAllow);
    const requests = this.initialized ? this.waiting.arrived(all) : all;
    if (!this.initialized) this.waiting.arrived(all);
    for (const request of requests) {
      const session = sessions.find((s) => s.id === request.session)!;
      remember({
        session: session.id,
        kind: session.permission?.questions.length || session.phase === "waitingInput" ? "question" : "permission",
        id: request.id ?? request.wait,
        at: session.permission?.since ?? session.phaseSince,
      });
    }
    for (const s of sessions) {
      if (s.startedAt !== undefined)
        remember({ session: s.id, kind: "started", id: String(s.startedAt), at: s.startedAt });
      const terminal = terminalSound(s);
      if (terminal) remember(terminal);
    }
    this.initialized = true;
    return result;
  }
}

export function terminalSound(s: Session): SessionSoundEvent | null {
  if (s.phase === "generating" || s.phase === "runningTool" || isWaiting(s.phase)) return null;
  const began = s.turnStarted ?? -Infinity;
  // Limit wins even when an agent also records a generic failed-turn marker.
  if (s.phase === "limited" && s.limitStop && s.limitStop.at >= began)
    return { session: s.id, kind: "limit", id: String(s.limitStop.at), at: s.limitStop.at };
  if (s.failedAt !== undefined && s.failedAt >= began && s.turnEndedAt !== undefined && s.failedAt <= s.turnEndedAt)
    return { session: s.id, kind: "error", id: String(s.failedAt), at: s.failedAt };
  if (s.turnCompleted && s.turnEndedAt !== undefined && s.turnEndedAt >= began)
    return { session: s.id, kind: "finished", id: String(s.turnEndedAt), at: s.turnEndedAt };
  return null;
}

/** One sound for a one-second burst (#232), highest priority first. Events stay
 * visible in the session UI; only their audio coalesces. The same 1 s window is
 * used by the Swift implementation. No permanent polling or overlapping checks. */
export const SOUND_BURST_MS = 1000;
const PRIORITY: Record<SoundEventKind, number> = {
  limit: 5,
  error: 4,
  permission: 3,
  question: 2,
  finished: 1,
  started: 0,
};
export interface SoundContext {
  prefs: SessionSoundPrefs;
  enabled: boolean;
  inView(session: string): boolean;
  current(event: SessionSoundEvent): boolean;
}
export class SessionSoundQueue {
  private pending: SessionSoundEvent[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private checking = false;
  private disposed = false;
  constructor(
    private readonly context: () => SoundContext,
    private readonly terminalShown: (ids: string[]) => Promise<boolean | undefined>,
    private readonly play: (sound: string, volume: number) => unknown,
    private readonly now = () => Date.now() / 1000,
  ) {}

  enqueue(events: SessionSoundEvent[]) {
    if (this.disposed) return;
    this.pending.push(...events.filter((event) => this.audible(event)));
    // A bounded batch, even when an integration floods updates.
    this.pending = this.pending.slice(-256);
    if (this.pending.length && !this.timer && !this.checking)
      this.timer = setTimeout(() => void this.flush(), SOUND_BURST_MS);
  }

  private audible(event: SessionSoundEvent) {
    const state = this.context();
    return (
      state.enabled &&
      !soundsMuted(state.prefs, this.now()) &&
      state.prefs.soundVolume > 0 &&
      state.prefs[SOUND_KEYS[event.kind]] !== "none" &&
      !state.inView(event.session) &&
      state.current(event)
    );
  }

  private async flush() {
    this.timer = undefined;
    this.checking = true;
    const batch = this.pending;
    this.pending = [];
    try {
      // Check each session separately: another session being hidden must never
      // make the focused session's higher-priority event audible.
      const sessions = [...new Set(batch.filter((event) => this.audible(event)).map((event) => event.session))];
      const checks = await Promise.all(
        sessions.map(async (id) => [id, await shouldPlay([id], this.terminalShown)] as const),
      );
      const unseen = new Map(checks);
      const choice = batch
        .filter((event) => unseen.get(event.session) && this.audible(event))
        .sort((a, b) => PRIORITY[b.kind] - PRIORITY[a.kind] || b.at - a.at)[0];
      if (choice && !this.disposed) {
        const prefs = this.context().prefs;
        await this.play(prefs[SOUND_KEYS[choice.kind]], prefs.soundVolume);
      }
    } catch {
      // Missing audio devices and failed IPC must not break session updates.
    } finally {
      this.checking = false;
      if (!this.disposed && this.pending.length) this.timer = setTimeout(() => void this.flush(), SOUND_BURST_MS);
    }
  }

  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
    this.pending = [];
  }
}
