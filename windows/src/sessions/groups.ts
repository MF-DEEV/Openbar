// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sessions grouped by project or by agent, and the branch each one works on
// (#233; NotchSessionGroups.swift).

import type { Grouping } from "../shared/prefs";
import { worktreeName } from "./worktree";
import { isWaiting, PROVIDER_NAMES, type Provider, type Session } from "./types";

export interface SessionGroup {
  /** Stable across restarts, so a collapsed group stays collapsed. */
  key: string;
  kind: "project" | "agent";
  /** The project's or the agent's name. */
  title: string;
  /** The agent, for a group of one agent's sessions. */
  provider?: Provider;
  /** The SSH alias, for a project on another machine. */
  remote?: string;
  /** In the list's order. */
  sessions: Session[];
  /** How many of them wait on the person. */
  waiting: number;
}

/** The key a session's group goes by. A project on another machine is its own group, under its host. */
export function groupKey(session: Session, by: Exclude<Grouping, "none">): string {
  if (by === "agent") return `agent:${session.provider}`;
  return `project:${session.remote ?? ""}:${session.project}`;
}

/**
 * The sessions in groups. `ordered` is the list as it shows ungrouped; the
 * groups come in the order their first session does there, so every group
 * with someone waiting is on top, and each keeps the list's order inside.
 */
export function groupSessions(ordered: readonly Session[], by: Exclude<Grouping, "none">): SessionGroup[] {
  const groups = new Map<string, SessionGroup>();
  for (const session of ordered) {
    const key = groupKey(session, by);
    let group = groups.get(key);
    if (!group) {
      group =
        by === "agent"
          ? {
              key,
              kind: "agent",
              title: PROVIDER_NAMES[session.provider],
              provider: session.provider,
              sessions: [],
              waiting: 0,
            }
          : { key, kind: "project", title: session.project, remote: session.remote, sessions: [], waiting: 0 };
      groups.set(key, group);
    }
    group.sessions.push(session);
    if (isWaiting(session.phase)) group.waiting += 1;
  }
  return [...groups.values()];
}

/** How many folded groups are remembered; the oldest go first. */
export const COLLAPSED_LIMIT = 64;

/** Collapsed groups as stored: keys of groups that are gone stay, so they come back collapsed. */
export function toggleCollapsed(collapsed: readonly string[], key: string): string[] {
  if (collapsed.includes(key)) return collapsed.filter((k) => k !== key);
  return [...collapsed, key].slice(-COLLAPSED_LIMIT);
}

export interface BranchLabel {
  /** The branch; null on a detached HEAD. */
  branch: string | null;
  /** The worktree's name, unless the branch already says it. */
  worktree: string | null;
  /** The session runs in a worktree an agent or the island made. */
  inWorktree: boolean;
  /** Everything, uncut, for the pointer and screen readers. */
  full: string;
}

/**
 * What the row says of where the session works: its branch, and its
 * worktree's name when the branch doesn't already carry it (`worktree-<name>`
 * from Claude Code, `openbar/<name>` from the island). Nothing on a detached
 * HEAD or outside git, unless a worktree names the checkout.
 */
export function branchLabel(session: Pick<Session, "branch" | "cwd">): BranchLabel | null {
  const branch = session.branch?.trim() || null;
  const worktree = worktreeName(session.cwd);
  if (!branch && !worktree) return null;
  const named =
    !!branch &&
    !!worktree &&
    (branch === worktree || branch.endsWith(`-${worktree}`) || branch.endsWith(`/${worktree}`));
  const shownWorktree = worktree && !named ? worktree : null;
  const full = [shownWorktree, branch].filter(Boolean).join(" · ");
  return { branch, worktree: shownWorktree, inWorktree: !!worktree, full };
}

/**
 * A name cut in the middle, so its start (the kind of work) and its end (the
 * topic) stay: `feat/…-checkout`. Counted in characters; the label's own
 * ellipsis catches what a narrow row still can't fit.
 */
export function cutMiddle(text: string, max: number): string {
  const chars = [...text];
  if (chars.length <= max || max < 3) return text;
  const keep = max - 1;
  const head = Math.ceil(keep / 2);
  const tail = keep - head;
  return `${chars.slice(0, head).join("")}…${chars.slice(chars.length - tail).join("")}`;
}
