// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// When a session has waited a while and nobody reacted, the island reminds
// the person once more (#231). The sessions on hand are the authority: a
// wait answered in the island or in the terminal is gone from them, so its
// reminder goes with it. The same as the Mac's NotchSessionReminders.swift.

import { isWaiting, type Session } from "./types";
import { samePrompt, shouldPlay, type WaitingRequest } from "./waitingSound";
import { SOUND_KEYS, soundsMuted, type SessionSoundPrefs, type SoundEventKind } from "./soundPreferences";

export type ReminderKind = "permission" | "question" | "finished";

/** Minutes before the first reminder, and between repeats (0 is once). */
export const REMIND_AFTER = [2, 5, 10, 15] as const;
export const REMIND_EVERY = [0, 5, 10, 15, 30] as const;

/** How often the waits are read. */
export const REMINDER_CHECK_MS = 15_000;
/** How long after an unlock a held reminder comes, so the island is back and the away card has opened first. */
export const REMINDER_AFTER_UNLOCK_MS = 2_000;
/** The longest the terminals are asked. */
export const REMINDER_CHECK_LIMIT_MS = 3_000;
/** A finished turn nobody looked at counts this long (seconds): read in the
 * terminal, a turn never shows as looked at, so an old one is let go. */
export const FINISHED_FOR = 60 * 60;

export interface ReminderPrefs {
  remindPermission: boolean;
  remindQuestion: boolean;
  remindFinished: boolean;
  remindAfter: number;
  remindEvery: number;
  remindDuringDnd: boolean;
}

export interface ReminderSettings {
  permission: boolean;
  question: boolean;
  finished: boolean;
  /** Seconds before the first reminder. */
  after: number;
  /** Seconds between repeats; null for once. */
  every: number | null;
  duringDnd: boolean;
}

export function reminderSettings(prefs: ReminderPrefs): ReminderSettings {
  return {
    permission: prefs.remindPermission,
    question: prefs.remindQuestion,
    finished: prefs.remindFinished,
    after: Math.max(1, prefs.remindAfter) * 60,
    every: prefs.remindEvery > 0 ? prefs.remindEvery * 60 : null,
    duringDnd: prefs.remindDuringDnd,
  };
}

export const anyReminderOn = (s: ReminderSettings) => s.permission || s.question || s.finished;

/** One thing a session waits on the person for, and since when (seconds). */
export interface ReminderWait {
  session: string;
  kind: ReminderKind;
  /** The prompt, as the sounds and attention name it, or the turn's end. */
  request: WaitingRequest;
  since: number;
}

const sameWait = (a: ReminderWait, b: ReminderWait) =>
  a.session === b.session && a.kind === b.kind && samePrompt(a.request, b.request);

/**
 * When the turn the person has not looked at yet ended. Not the "finished"
 * phase, which turns idle after a while by itself: a turn stays unseen until
 * its page is opened or the next one begins.
 */
export function unseenTurnEnd(session: Session): number | null {
  if (session.phase !== "finished" && session.phase !== "idle") return null;
  if (session.background || !session.turnCompleted || session.turnEndedAt === undefined) return null;
  const began = session.turnStarted ?? -Infinity;
  const ended = session.turnEndedAt;
  if (ended < began) return null;
  if (session.lookedAt !== undefined && session.lookedAt >= ended) return null;
  // A turn that stopped on an error is told by its own sound, not this.
  if (session.failedAt !== undefined && session.failedAt >= began) return null;
  return ended;
}

/** What the session waits on the person for now, if anything. */
export function reminderWait(session: Session, now: number): ReminderWait | null {
  if (isWaiting(session.phase)) {
    const question = session.phase === "waitingInput" || (session.permission?.questions.length ?? 0) > 0;
    return {
      session: session.id,
      kind: question ? "question" : "permission",
      request: {
        session: session.id,
        wait: `${session.phase}:${session.phaseSince}`,
        id: session.permission?.requestId ?? null,
      },
      since: session.permission?.since ?? session.phaseSince,
    };
  }
  const ended = unseenTurnEnd(session);
  if (ended === null || now - ended >= FINISHED_FOR) return null;
  return {
    session: session.id,
    kind: "finished",
    request: { session: session.id, wait: `finished:${ended}`, id: null },
    since: ended,
  };
}

export class SessionReminders {
  /** Each session's wait and when it was last reminded of (or seen in its own terminal). */
  private reminded = new Map<string, { wait: ReminderWait; at: number }>();

  /**
   * The waits that call for a reminder now, oldest first. A wait already due
   * brings along every other one that has waited at least half the time, so
   * sessions that wait together give one reminder, not a burst.
   */
  due(sessions: readonly Session[], settings: ReminderSettings, now: number): ReminderWait[] {
    const waits = sessions.map((session) => reminderWait(session, now)).filter((w): w is ReminderWait => w !== null);
    // A wait that ended, or turned into another, forgets its reminder.
    for (const [id, entry] of this.reminded)
      if (!waits.some((wait) => wait.session === id && sameWait(wait, entry.wait))) this.reminded.delete(id);
    if (!anyReminderOn(settings)) return [];
    const wanted = waits.filter((wait) => settings[wait.kind] && wait.since <= now);
    // `slack` lets a wait come along a little early with one that is due.
    const ready = (wait: ReminderWait, slack: number) => {
      const last = this.reminded.get(wait.session)?.at;
      if (last !== undefined) return settings.every !== null && now - last >= settings.every - slack;
      return now - wait.since >= settings.after - slack;
    };
    if (!wanted.some((wait) => ready(wait, 0))) return [];
    return wanted.filter((wait) => ready(wait, settings.after / 2)).sort((a, b) => a.since - b.since);
  }

  /** Notes the waits as reminded of, or as seen in their own terminal. */
  mark(waits: readonly ReminderWait[], now: number) {
    for (const wait of waits) this.reminded.set(wait.session, { wait, at: now });
  }

  /** Whether the wait is still what the session waits on. */
  static current(wait: ReminderWait, sessions: readonly Session[], now: number): boolean {
    const session = sessions.find((s) => s.id === wait.session);
    const nowWait = session ? reminderWait(session, now) : null;
    return nowWait !== null && sameWait(nowWait, wait);
  }
}

const SOUND_PRIORITY: Record<ReminderKind, number> = { permission: 3, question: 2, finished: 1 };

/** The sound of the most pressing wait, as each event plays it (#262). */
export function reminderSound(waits: readonly ReminderWait[]): SoundEventKind | null {
  let best: ReminderWait | null = null;
  for (const wait of waits) if (!best || SOUND_PRIORITY[wait.kind] > SOUND_PRIORITY[best.kind]) best = wait;
  return best?.kind ?? null;
}

/** Whole minutes a session has waited on a permission or a question, from one minute on. */
export function waitedMinutes(session: Session, now: number): number | null {
  if (!isWaiting(session.phase)) return null;
  const minutes = Math.floor((now - (session.permission?.since ?? session.phaseSince)) / 60);
  return minutes >= 1 ? minutes : null;
}

/** The reminder waits for Do Not Disturb to end, unless allowed. */
export const heldByDnd = (settings: ReminderSettings, dnd: boolean | null | undefined) =>
  !settings.duringDnd && dnd === true;

export interface ReminderDeps {
  /** The sessions the person can see: Codex requests answered by themselves are left out. */
  sessions(): readonly Session[];
  prefs(): ReminderPrefs & SessionSoundPrefs;
  /** The island and its Sessions section are on. */
  enabled(): boolean;
  /** Locked or display off, and Do Not Disturb, from the system. */
  quiet(): Promise<{ locked: boolean; dnd: boolean | null } | undefined>;
  /** The lock the away watcher heard (#215). */
  locked(): boolean;
  terminalShown(ids: string[]): Promise<boolean | undefined>;
  play(sound: string, volume: number): unknown;
  present(waits: ReminderWait[]): unknown;
  now(): number;
}

/**
 * Reads the waits and reminds of the ones due (#231). A locked screen, a
 * display off, and Do Not Disturb (unless allowed) hold the reminder until
 * they end; a session whose own terminal tab is in front counts as seen. One
 * check at a time, so a slow terminal never stacks reminders.
 */
export class ReminderRunner {
  private reminders = new SessionReminders();
  private checking = false;
  constructor(private readonly deps: ReminderDeps) {}

  /** The waits reminded of, for tests and the log. */
  async check(): Promise<ReminderWait[]> {
    const { deps } = this;
    if (this.checking || !deps.enabled() || deps.locked()) return [];
    const settings = reminderSettings(deps.prefs());
    const due = this.reminders.due(deps.sessions(), settings, deps.now());
    if (due.length === 0) return [];
    this.checking = true;
    try {
      const quiet = await deps.quiet().catch(() => undefined);
      if (deps.locked() || quiet?.locked) return [];
      if (heldByDnd(settings, quiet?.dnd)) return [];
      const checks = await Promise.all(
        due.map(async (wait) => {
          const unseen = await shouldPlay([wait.session], deps.terminalShown, REMINDER_CHECK_LIMIT_MS);
          return { wait, shown: !unseen };
        }),
      );
      // Answered while the terminals were asked: no reminder of it.
      const now = deps.now();
      const sessions = deps.sessions();
      const live = checks.filter(({ wait }) => SessionReminders.current(wait, sessions, now));
      this.reminders.mark(
        live.filter((c) => c.shown).map((c) => c.wait),
        now,
      );
      const remind = live.filter((c) => !c.shown).map((c) => c.wait);
      if (remind.length === 0 || !deps.enabled()) return [];
      this.reminders.mark(remind, now);
      const prefs = deps.prefs();
      const kind = reminderSound(remind);
      if (kind && !soundsMuted(prefs, now) && prefs.soundVolume > 0 && prefs[SOUND_KEYS[kind]] !== "none")
        await Promise.resolve(deps.play(prefs[SOUND_KEYS[kind]], prefs.soundVolume)).catch(() => {});
      await Promise.resolve(deps.present(remind));
      return remind;
    } finally {
      this.checking = false;
    }
  }
}
