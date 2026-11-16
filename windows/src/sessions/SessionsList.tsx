// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions page: one row per running Claude Code session, the ones
// waiting on the person first (NotchClaudeSessionsView.swift; spec §4.3).

import { ModelLabel } from "./progress/ModelLabel";
import { RowBar, TaskCount } from "./progress/TaskProgress";
import { useLocale } from "../i18n";

import { Check, ChevronRight, X } from "lucide-react";
import { AgentFace } from "../shared/ui/AgentFace";
import { AnimatePresence, motion } from "motion/react";
import type { CSSProperties } from "react";
import { Collapse } from "../shared/ui/motionKit";
import { BranchChip, GroupHeader } from "./SessionGroups";
import { groupSessions } from "./groups";
import { useProto233 } from "./proto233";
import { EmptyState } from "../island/views/Controls";
import { SessionAvatar } from "./SessionAvatar";
import {
  activityText,
  displayName,
  dozing,
  phaseClock,
  phaseTint,
  projectColor,
  providerTint,
  sessionPhaseLabel,
} from "./format";
import { useNow } from "./useNow";
import { AccountMark, BackgroundMark, RemoteHostMark } from "./Background";
import { S } from "./strings";
import { settled, useSessions } from "./store";
import { isWaiting, type Session } from "./types";
import { presentedSnapshot } from "./presentation";
import { useAgents } from "../agents/store";
import { limitReset, resetText } from "./limits";

/** waitingPermission 0 · waitingInput 1 · working 2 · limited 3 · background 4 · finished 5 · idle 6 (§3.6). */
const RANK: Record<Session["phase"], number> = {
  waitingPermission: 0,
  waitingInput: 1,
  generating: 2,
  runningTool: 2,
  limited: 3,
  background: 4,
  finished: 5,
  idle: 6,
};

export function orderSessions(sessions: Session[]): Session[] {
  return [...sessions].sort((a, b) => {
    const ra = RANK[a.phase];
    const rb = RANK[b.phase];
    if (ra !== rb) return ra - rb;
    let d = 0;
    if (ra <= 1) d = a.phaseSince - b.phaseSince;
    else if (ra === 2) d = (a.turnStarted ?? a.phaseSince) - (b.turnStarted ?? b.phaseSince);
    else if (ra <= 5) d = b.phaseSince - a.phaseSince;
    else d = (b.lastActive ?? 0) - (a.lastActive ?? 0);
    if (d !== 0) return d;
    return a.project.localeCompare(b.project) || a.pid - b.pid;
  });
}

/** Spring(response 0.35, damping 0.85) for reordering rows. */
const REORDER = { type: "spring", stiffness: 322.3, damping: 30.5 } as const;

export function SessionsList() {
  useLocale();
  const sessions = useSessions((s) => presentedSnapshot(s).sessions);
  const grouping = useSessions((s) => s.prefs.grouping);
  const now = useNow();
  if (sessions.length === 0) {
    return <EmptyState icon={AgentFace} title={S.emptyTitle} detail={S.emptyDetail} />;
  }
  if (grouping !== "none") return <GroupedList sessions={sessions} by={grouping} now={now} />;
  return (
    <div className="s-list">
      <AnimatePresence initial={false}>
        {orderSessions(sessions).map((session) => (
          <motion.div
            key={session.id}
            layout
            transition={REORDER}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
          >
            <Row session={session} now={now} />
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}

/** The list in groups (#233): groups with someone waiting on top, each folding its rows away. */
function GroupedList({ sessions, by, now }: { sessions: Session[]; by: "project" | "agent"; now: number }) {
  const collapsed = useSessions((s) => s.prefs.collapsedGroups);
  const look = useProto233((s) => s.look);
  const groups = groupSessions(orderSessions(sessions), by);
  return (
    <div className={`s-list is-grouped is-look-${look}`}>
      <AnimatePresence initial={false}>
        {groups.map((group) => {
          const folded = collapsed.includes(group.key);
          const tint =
            group.kind === "agent" && group.provider ? providerTint(group.provider) : projectColor(group.title);
          return (
            <motion.section
              key={group.key}
              layout="position"
              transition={REORDER}
              className={`s-group${folded ? " is-collapsed" : ""}`}
              style={{ "--s-group-tint": tint } as CSSProperties}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              <GroupHeader group={group} collapsed={folded} />
              <Collapse open={!folded}>
                <div className="s-group-rows">
                  <AnimatePresence initial={false}>
                    {group.sessions.map((session) => (
                      <motion.div
                        key={session.id}
                        layout="position"
                        transition={REORDER}
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                      >
                        <Row session={session} now={now} inProject={by === "project"} />
                      </motion.div>
                    ))}
                  </AnimatePresence>
                </div>
              </Collapse>
            </motion.section>
          );
        })}
      </AnimatePresence>
    </div>
  );
}

function Row({ session, now, inProject = false }: { session: Session; now: number; inProject?: boolean }) {
  useLocale();
  const custom = useSessions((s) => s.names[session.id]);
  const lookedAt = useSessions((s) => s.lookedAt[session.id]);
  const buddies = useSessions((s) => s.prefs.buddies);
  const name = displayName(session, custom);
  const tint = phaseTint(session.phase, session.provider);
  const time = phaseClock(session, now);
  // When a session the plan's limit stopped goes on: as the agent said, or as the limits reading has it.
  const agents = useAgents((s) => s.snapshot);
  const locale = useLocale();
  const resumes = session.phase === "limited" ? limitReset(session, agents, now) : null;
  const activity = activityText(session, name, resumes ? resetText(resumes, now, locale) : null);
  const limited = session.phase === "limited";
  const quick = session.permission?.fitsOnALine && session.permission.questions.length === 0;
  const color = projectColor(session.project);
  const waiting = isWaiting(session.phase);
  // A session whose Clawd sleeps fades back a little; sessions.css brings it
  // forward under the pointer or the keyboard, and keeps it whole with more
  // contrast asked for.
  const asleep = buddies && dozing(session, now, lookedAt);

  return (
    <div
      role="button"
      tabIndex={0}
      className={`s-row${waiting ? " is-waiting" : ""}${limited ? " is-limited" : ""}${asleep ? " is-dozing" : ""}`}
      aria-label={`${name}, ${session.provider === "codex" ? S.codex : S.claudeCode}, ${session.project}, ${sessionPhaseLabel(session)}`}
      onClick={() => useSessions.getState().focus(session.id)}
      onKeyDown={(e) => e.key === "Enter" && useSessions.getState().focus(session.id)}
    >
      <SessionAvatar session={session} now={now} lookedAt={lookedAt} size="row" agentMark ring />
      <div className="s-row-text">
        <div className="s-row-line1">
          <span className="s-row-name">{name}</span>
          {session.background ? <BackgroundMark /> : null}
          {session.remote ? <RemoteHostMark alias={session.remote} /> : null}
          {session.accountLabel ? <AccountMark name={session.accountLabel} /> : null}
          <ModelLabel session={session} />
          <TaskCount session={session} />
          {time ? (
            <span className="s-clock" style={{ color: tint }}>
              {time}
            </span>
          ) : null}
        </div>
        <div className="s-row-line2">
          {/* In a project's group the header names the project; the row keeps its branch. */}
          {inProject ? null : (
            <span className="s-tag" style={{ color, background: `color-mix(in srgb, ${color} 16%, transparent)` }}>
              {session.project}
            </span>
          )}
          <BranchChip session={session} />
          <span className="s-activity" style={waiting || limited ? { color: tint } : undefined}>
            {activity}
          </span>
        </div>
      </div>
      <RowBar session={session} />
      {quick ? <QuickDecision session={session} /> : <ChevronRight className="s-chevron" size={11} strokeWidth={2.5} />}
    </div>
  );
}

function QuickDecision({ session }: { session: Session }) {
  useLocale();
  const ready = useSessions((s) => settled(s, session));
  const decide = useSessions((s) => s.decide);
  return (
    <div className="s-quick" onClick={(e) => e.stopPropagation()}>
      <button
        type="button"
        className="s-quick-button is-deny"
        aria-label={S.deny}
        title={S.deny}
        disabled={!ready}
        onClick={() => void decide(session, "deny")}
      >
        <X size={12} strokeWidth={3} />
      </button>
      <button
        type="button"
        className="s-quick-button is-allow"
        aria-label={S.allow}
        title={S.allow}
        disabled={!ready}
        onClick={() => void decide(session, "allow")}
      >
        <Check size={12} strokeWidth={3} />
      </button>
    </div>
  );
}
