// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions page: one row per running Claude Code session, the ones
// waiting on the person first (NotchClaudeSessionsView.swift; spec §4.3).

import { useLocale } from "../i18n";

import { Check, ChevronRight, GalleryVerticalEnd, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { EmptyState } from "../island/views/Controls";
import { SessionAvatar } from "./SessionAvatar";
import { activityText, displayName, dozing, phaseClock, phaseTint, projectColor } from "./format";
import { useNow } from "./useNow";
import { S } from "./strings";
import { settled, useSessions } from "./store";
import { isWaiting, type Session } from "./types";
import { presentedSnapshot } from "./presentation";

/** waitingPermission 0 · waitingInput 1 · working 2 · background 3 · finished 4 · idle 5 (§3.6). */
const RANK: Record<Session["phase"], number> = {
  waitingPermission: 0,
  waitingInput: 1,
  generating: 2,
  runningTool: 2,
  background: 3,
  finished: 4,
  idle: 5,
};

export function orderSessions(sessions: Session[]): Session[] {
  return [...sessions].sort((a, b) => {
    const ra = RANK[a.phase];
    const rb = RANK[b.phase];
    if (ra !== rb) return ra - rb;
    let d = 0;
    if (ra <= 1) d = a.phaseSince - b.phaseSince;
    else if (ra === 2) d = (a.turnStarted ?? a.phaseSince) - (b.turnStarted ?? b.phaseSince);
    else if (ra <= 4) d = b.phaseSince - a.phaseSince;
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
  const now = useNow();
  if (sessions.length === 0) {
    return <EmptyState icon={GalleryVerticalEnd} title={S.emptyTitle} detail={S.emptyDetail} />;
  }
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

function Row({ session, now }: { session: Session; now: number }) {
  useLocale();
  const custom = useSessions((s) => s.names[session.id]);
  const lookedAt = useSessions((s) => s.lookedAt[session.id]);
  const buddies = useSessions((s) => s.prefs.buddies);
  const name = displayName(session, custom);
  const tint = phaseTint(session.phase, session.provider);
  const time = phaseClock(session, now);
  const activity = activityText(session, name);
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
      className={`s-row${waiting ? " is-waiting" : ""}${asleep ? " is-dozing" : ""}`}
      aria-label={`${name}, ${session.provider === "codex" ? "Codex" : "Claude Code"}, ${session.project}, ${S[session.phase]}`}
      onClick={() => useSessions.getState().focus(session.id)}
      onKeyDown={(e) => e.key === "Enter" && useSessions.getState().focus(session.id)}
    >
      <SessionAvatar session={session} now={now} lookedAt={lookedAt} size="row" agentMark />
      <div className="s-row-text">
        <div className="s-row-line1">
          <span className="s-row-name">{name}</span>
          {time ? (
            <span className="s-clock" style={{ color: tint }}>
              {time}
            </span>
          ) : null}
        </div>
        <div className="s-row-line2">
          <span className="s-tag" style={{ color, background: `color-mix(in srgb, ${color} 16%, transparent)` }}>
            {session.project}
          </span>
          <span className="s-activity" style={waiting ? { color: tint } : undefined}>
            {activity}
          </span>
          {session.branch ? <span className="s-branch">⎇ {session.branch}</span> : null}
        </div>
      </div>
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
