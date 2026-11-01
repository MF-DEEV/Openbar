// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The plan limits the Sessions section shows on the island
// (NotchSessionLimitsSupport.swift). They come from the reading the AI
// section keeps (agent_usage); nothing here reads a file or asks an agent.
// Limit times are milliseconds, as agent_usage sends them; session times are
// seconds, as the sessions service sends them.

import { displayedReset, restingLimit } from "../agents/model";
import type { AgentSnapshot, LimitDisplay, LimitFocus, LimitWindow, Provider as AgentProvider } from "../agents/types";
import { percent } from "../agents/format";
import { isActive, isWaiting, type Session } from "./types";

/** The agents Sessions follows that report plan limits. */
export const LIMIT_PROVIDERS = ["claude", "codex"] as const;

/** The limit the closed strip shows beside the counts: the window the person
 * chose for the AI strip, or the most used one, among the agents with
 * sessions listed, once it passes the warning threshold. A chosen window
 * still under it gives way to one past it, as a weekly cap reached with room
 * left in the session window. */
export function stripLimit(
  snapshot: AgentSnapshot,
  agents: ReadonlySet<string>,
  focus: LimitFocus,
  threshold: number,
  now: number,
): { provider: AgentProvider; window: LimitWindow } | null {
  const scoped: AgentSnapshot = {
    ...snapshot,
    limits: Object.fromEntries(
      Object.entries(snapshot.limits).filter(
        ([provider]) => agents.has(provider) && (LIMIT_PROVIDERS as readonly string[]).includes(provider),
      ),
    ),
    codexAccounts: [],
  };
  const chosen = restingLimit(scoped, focus, now);
  const most = restingLimit(scoped, "mostUsed", now);
  return [chosen, most].find((limit) => limit && limit.window.usedPercent >= threshold) ?? null;
}

/** The share the strip writes, as the person reads limits. */
export function stripShare(window: LimitWindow, display: LimitDisplay): string {
  const used = Math.min(1, Math.max(0, window.usedPercent / 100));
  return percent(display === "used" ? used : 1 - used);
}

/** A window this full has run out, allowing for rounding in the reading. */
export const SPENT_PERCENT = 99.5;

/** When the limit that stopped a session lifts, in seconds: as the agent
 * said, or else the latest reset among its agent's spent windows of that
 * kind, since the work waits for all of them. Null once that time passed or
 * while nothing tells it. */
export function limitReset(session: Session, snapshot: AgentSnapshot, nowSeconds: number): number | null {
  const stop = session.limitStop;
  if (!stop) return null;
  if (stop.resetsAt !== undefined) return stop.resetsAt > nowSeconds ? stop.resetsAt : null;
  const windows = snapshot.limits[session.provider]?.windows ?? [];
  const resets = windows
    .filter((w) => w.usedPercent >= SPENT_PERCENT && (stop.kind === undefined || w.kind === stop.kind))
    .map((w) => displayedReset(w, nowSeconds * 1000))
    .filter((r): r is number => r !== null);
  return resets.length ? Math.max(...resets) / 1000 : null;
}

/** A reset time as short as it can be told: the time today, the weekday and
 * time within the week, the date and time after that. */
export function resetText(atSeconds: number, nowSeconds: number, locale?: string, timeZone?: string): string {
  const date = new Date(atSeconds * 1000);
  const now = new Date(nowSeconds * 1000);
  const day = (d: Date) => new Intl.DateTimeFormat("en-CA", { timeZone, dateStyle: "short" }).format(d);
  const time: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit", timeZone };
  if (day(date) === day(now)) return new Intl.DateTimeFormat(locale, time).format(date);
  const days = (Date.parse(day(date)) - Date.parse(day(now))) / 86_400_000;
  return new Intl.DateTimeFormat(
    locale,
    days < 7 ? { ...time, weekday: "short" } : { ...time, month: "short", day: "numeric" },
  ).format(date);
}

/** The sessions of an agent still under way, at work or waiting on the
 * person, which a warning about that agent's limit counts. */
export function runningSessions(sessions: readonly Session[], provider: string): number {
  return sessions.filter((s) => s.provider === provider && (isActive(s.phase) || isWaiting(s.phase))).length;
}
