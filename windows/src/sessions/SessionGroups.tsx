// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The list's groups (#233): a header for each project or agent that folds
// its rows away, the switch between one list and the groups, and the branch
// label every row carries (NotchSessionGroupViews.swift).

import { useLocale } from "../i18n";

import { ChevronRight, Folder, GitBranch, List, FolderGit2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useId } from "react";
import { AgentFace } from "../shared/ui/AgentFace";
import { SlidingHighlight } from "../shared/ui/motionKit";
import { setPrefs, type Grouping } from "../shared/prefs";
import { RemoteHostMark } from "./Background";
import { phaseTint, projectColor, providerTint } from "./format";
import { HandFill, ProviderMark } from "./glyphs";
import { branchLabel, cutMiddle, toggleCollapsed, type SessionGroup } from "./groups";
import { S } from "./strings";
import { useSessions } from "./store";
import { isWaiting, type Session } from "./types";

/** Picks how the list is grouped, here and in Settings alike. */
export function setGrouping(grouping: Grouping) {
  useSessions.setState((s) => ({ prefs: { ...s.prefs, grouping } }));
  void setPrefs({ sessions: { grouping } });
}

/** Folds a group or opens it again; the choice outlives a restart. */
export function toggleGroup(key: string) {
  const collapsedGroups = toggleCollapsed(useSessions.getState().prefs.collapsedGroups, key);
  useSessions.setState((s) => ({ prefs: { ...s.prefs, collapsedGroups } }));
  void setPrefs({ sessions: { collapsedGroups } });
}

const CHOICES: { value: Grouping; Icon: typeof List; label: () => string }[] = [
  { value: "none", Icon: List, label: () => S.groupNone },
  { value: "project", Icon: Folder, label: () => S.groupByProject },
  { value: "agent", Icon: AgentFace, label: () => S.groupByAgent },
];

/** One list, by project, by agent: three marks in the band, one press each. */
export function GroupingSwitch() {
  useLocale();
  const grouping = useSessions((s) => s.prefs.grouping);
  const id = useId();
  return (
    <div className="s-grouping" role="radiogroup" aria-label={S.groupSessions}>
      {CHOICES.map(({ value, Icon, label }) => (
        <button
          key={value}
          type="button"
          role="radio"
          aria-checked={grouping === value}
          aria-label={label()}
          title={label()}
          className={grouping === value ? "is-on" : ""}
          onClick={() => setGrouping(value)}
        >
          {grouping === value ? <SlidingHighlight layoutId={id} className="s-grouping-pill" /> : null}
          <Icon size={12} strokeWidth={2.2} />
        </button>
      ))}
    </div>
  );
}

/** A project's first letter on its color, or the agent's own mark. */
function GroupMark({ group }: { group: SessionGroup }) {
  if (group.kind === "agent" && group.provider) {
    return (
      <span className="s-group-mark is-agent" style={{ color: providerTint(group.provider) }}>
        <ProviderMark provider={group.provider} size={11} />
      </span>
    );
  }
  const color = projectColor(group.title);
  const letter = [...group.title.trim()][0]?.toUpperCase() ?? "?";
  return (
    <span
      className="s-group-mark"
      style={{ color, background: `color-mix(in srgb, ${color} 22%, transparent)` }}
      aria-hidden
    >
      {letter}
    </span>
  );
}

/** A folded group's sessions as small faces in their phase's color, the waiting ones first. */
function FaceStack({ sessions }: { sessions: Session[] }) {
  const shown = sessions.slice(0, 4);
  const more = sessions.length - shown.length;
  return (
    <span className="s-group-faces" aria-hidden>
      {shown.map((session) => (
        <span
          key={session.id}
          className={`s-group-face${isWaiting(session.phase) ? " is-waiting" : ""}`}
          style={{ color: phaseTint(session.phase, session.provider) }}
        >
          <ProviderMark provider={session.provider} size={9} working={false} />
        </span>
      ))}
      {more > 0 ? <span className="s-group-more">+{more}</span> : null}
    </span>
  );
}

/** A group's header: what it is, how many, whether someone waits, and the fold. */
export function GroupHeader({ group, collapsed }: { group: SessionGroup; collapsed: boolean }) {
  useLocale();
  const reduce = useReducedMotion();
  const tint = group.kind === "agent" && group.provider ? providerTint(group.provider) : projectColor(group.title);
  const parts = [group.title, S.groupCount(group.sessions.length)];
  if (group.remote) parts.push(S.hostHelp(group.remote));
  if (group.waiting > 0) parts.push(S.groupWaiting(group.waiting));
  return (
    <button
      type="button"
      className={`s-group-head${collapsed ? " is-collapsed" : ""}${group.waiting > 0 ? " has-waiting" : ""}`}
      aria-expanded={!collapsed}
      aria-label={parts.join(", ")}
      onClick={() => toggleGroup(group.key)}
    >
      <ChevronRight className="s-group-chevron" size={11} strokeWidth={2.6} aria-hidden />
      <GroupMark group={group} />
      <span className="s-group-title" style={{ color: tint }}>
        {group.title}
      </span>
      {group.remote ? <RemoteHostMark alias={group.remote} /> : null}
      <span className="s-group-count">{group.sessions.length}</span>
      <span className="s-group-fill" />
      <AnimatePresence initial={false}>
        {collapsed ? (
          <motion.span
            key="faces"
            initial={reduce ? { opacity: 0 } : { opacity: 0, x: 6 }}
            animate={{ opacity: 1, x: 0 }}
            exit={reduce ? { opacity: 0 } : { opacity: 0, x: 6 }}
            transition={{ duration: 0.18, ease: "easeOut" }}
          >
            <FaceStack sessions={group.sessions} />
          </motion.span>
        ) : null}
      </AnimatePresence>
      <AnimatePresence initial={false}>
        {group.waiting > 0 ? (
          <motion.span
            key="waiting"
            className="s-group-waiting"
            initial={reduce ? { opacity: 0 } : { opacity: 0, scale: 0.5 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={reduce ? { opacity: 0 } : { opacity: 0, scale: 0.5 }}
            transition={reduce ? { duration: 0.12 } : { type: "spring", stiffness: 520, damping: 22 }}
          >
            <HandFill size={10} />
            {group.waiting}
          </motion.span>
        ) : null}
      </AnimatePresence>
    </button>
  );
}

/** The row's branch, cut in the middle, with the worktree's name when the branch doesn't carry it. */
export function BranchChip({ session, max = 26 }: { session: Pick<Session, "branch" | "cwd">; max?: number }) {
  useLocale();
  const label = branchLabel(session);
  if (!label) return null;
  const Icon = label.inWorktree ? FolderGit2 : GitBranch;
  return (
    <span className="s-branch-chip" title={label.full} aria-label={label.full}>
      <Icon size={9} strokeWidth={2.4} aria-hidden />
      {label.worktree ? <span className="s-branch-worktree">{cutMiddle(label.worktree, 16)}</span> : null}
      {label.worktree && label.branch ? <span className="s-branch-sep">·</span> : null}
      {label.branch ? <span className="s-branch-name">{cutMiddle(label.branch, max)}</span> : null}
    </span>
  );
}
