// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// What the sessions' own tray icon says and lists (#331), from the sessions
// as the island presents them: Codex turns that Always answers count as work,
// not as asks. Kept apart from the window so it can be tested.

import type { TrayAsk, TraySessionsModel } from "../shared/bridge";
import { displayName } from "./format";
import { presentedSnapshot } from "./presentation";
import { answersHere, permissionQueue, rowLine } from "./queue";
import { S } from "./strings";
import type { Session, Snapshot } from "./types";

/** A menu line is kept to this many characters. */
export const MENU_TITLE_LIMIT = 72;

/** One readable line: the first, cut with an ellipsis past the limit. */
export function menuTitle(title: string, limit = MENU_TITLE_LIMIT): string {
  const line = title.split(/\r?\n/)[0] ?? "";
  const characters = [...line];
  if (characters.length <= limit) return line;
  return (
    characters
      .slice(0, limit - 1)
      .join("")
      .trimEnd() + "…"
  );
}

/**
 * Every waiting ask, oldest first as the island's queue lists them: the
 * session's name, then the command or path. Allow and Deny go where the
 * queue answers in place; a question, a plan or a command to read whole
 * only opens, on its card.
 */
export function trayAsks(sessions: Session[], names: Record<string, string>): TrayAsk[] {
  return permissionQueue(sessions).rows.map((row) => {
    const line = rowLine(row.ask) || row.ask.tool;
    const title = [displayName(row.session, names[row.session.id]), line].filter(Boolean).join(" — ");
    return {
      session: row.session.id,
      request: row.ask.requestId,
      title: menuTitle(title),
      decides: answersHere(row.ask),
    };
  });
}

/** The count beside or on the icon: none for one ask, which the dot tells, then the number, then "9+". */
export function badgeText(waiting: number): string {
  if (waiting < 2) return "";
  return waiting > 9 ? "9+" : String(waiting);
}

export function trayModel(
  state: { snapshot: Snapshot; autoAllow: Record<string, boolean>; names: Record<string, string> },
  visible: boolean,
): TraySessionsModel {
  const presented = presentedSnapshot(state);
  const waiting = presented.counts.waiting;
  const status = S.trayWaiting(waiting);
  return {
    visible,
    waiting,
    name: S.trayName,
    // Windows 11 names the icon for Narrator by the tooltip it was added with
    // ("Openbar Sessions") followed by the one it has now, unless the two are
    // the same: the tooltip says the name at rest and only the state otherwise.
    tooltip: waiting > 0 ? status : S.trayName,
    label: badgeText(waiting),
    header: status,
    asks: trayAsks(presented.sessions, state.names),
    openSessions: S.trayOpenSessions,
    settings: S.traySettings,
    allow: S.allow,
    deny: S.deny,
    openAsk: S.trayOpenAsk,
  };
}

/** Waits new since the last reading: a session that started waiting, or another ask; the first reading is where it starts. */
export class NewAsks {
  private seen: Set<string> | null = null;

  /** True when something arrived to wait on the person that was not there before. */
  arrived(sessions: Session[]): boolean {
    const now = new Set(permissionQueue(sessions).rows.map((row) => row.id));
    for (const session of sessions)
      if (session.phase === "waitingPermission" || session.phase === "waitingInput")
        now.add(`${session.id}\u0001${session.phase}\u0001${session.phaseSince}`);
    const before = this.seen;
    this.seen = now;
    if (!before) return false;
    for (const id of now) if (!before.has(id)) return true;
    return false;
  }
}
