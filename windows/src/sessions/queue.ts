// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every ask the island can answer, across sessions, in one list (#269):
// "3 waiting", each ask on its own row, and Allow all for the ones that look
// safe. Oldest first, so a new ask joins at the foot and the row being read
// never moves. Mirrors NotchPermissionQueue.swift.

import type { Permission, Session } from "./types";

/** The plan tool, approved into a mode on its card. */
const PLAN_TOOL = "ExitPlanMode";

/** Shown once more than one ask waits; one ask keeps its own page. */
export const QUEUE_THRESHOLD = 2;

export interface QueueRow {
  /** Session id and request id, steady while the row is listed. */
  id: string;
  session: Session;
  ask: Permission;
}

/** Every ask the island can answer for a session, oldest first. */
export function asks(session: Session): Permission[] {
  return session.permission ? [session.permission, ...(session.laterAsks ?? [])] : [];
}

/** The session as its page shows it with this ask on the card: the chosen one first, the others behind it. */
export function showing(session: Session, requestId: string | null | undefined): Session {
  if (!requestId || session.permission?.requestId === requestId) return session;
  const all = asks(session);
  const chosen = all.find((p) => p.requestId === requestId);
  if (!chosen) return session;
  return { ...session, permission: chosen, laterAsks: all.filter((p) => p.requestId !== requestId) };
}

/** Answered from the row, without opening its page: not a question, a plan, or a command to read whole first. */
export function answersHere(ask: Permission): boolean {
  return (
    ask.questions.length === 0 &&
    ask.tool !== PLAN_TOOL &&
    ask.command?.requiresInspection !== true &&
    ask.command?.complete !== false
  );
}

/** Always on the row: Codex's for the rest of its turn, any other agent's where it offers rules. */
export function offersAlways(row: QueueRow): boolean {
  return answersHere(row.ask) && (row.session.provider === "codex" || row.ask.offersAlways);
}

/** Allow all takes it: answered here, and nothing about it looks risky. */
export function joinsAllowAll(row: QueueRow): boolean {
  return answersHere(row.ask) && row.ask.risk === "safe";
}

/** What the row shows of the ask on one line: the command or path. */
export function rowLine(ask: Permission): string {
  const first = ask.detail.split(/\r?\n/)[0]?.trim();
  return first || ask.summary?.trim() || "";
}

export interface PermissionQueue {
  rows: QueueRow[];
  count: number;
  shows: boolean;
  allowAll: QueueRow[];
  /** Asks Allow all leaves for the person, one by one. */
  heldBack: number;
  /** The ask a key answers when no page is open: the oldest one that is not a question. */
  keyTarget?: QueueRow;
}

export function permissionQueue(sessions: Session[]): PermissionQueue {
  const rows = sessions
    .flatMap((session) => asks(session).map((ask) => ({ id: `${session.id}\u0001${ask.requestId}`, session, ask })))
    .map((row, order) => ({ row, order }))
    // Oldest first; asks of one moment keep the sessions' order.
    .sort((a, b) => a.row.ask.since - b.row.ask.since || a.order - b.order)
    .map(({ row }) => row);
  const allowAll = rows.filter(joinsAllowAll);
  return {
    rows,
    count: rows.length,
    shows: rows.length >= QUEUE_THRESHOLD,
    allowAll,
    heldBack: rows.length - allowAll.length,
    keyTarget: rows.find((r) => r.ask.questions.length === 0),
  };
}
