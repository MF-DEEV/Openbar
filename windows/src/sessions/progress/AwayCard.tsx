// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// "While you were away" (#215): on return the island opens on one card, a
// tile per session that changed. The island's top row holds how long the
// person was gone, a tally, and a close button that counts down.

import { CircleX, MoonStar } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type CSSProperties, type ReactElement, useEffect } from "react";
import { create } from "zustand";
import { useLocale } from "../../i18n";
import { useIsland } from "../../island/store";
import type { AwayLine, Outcome } from "../away";
import { CheckCircleFill, GaugeFill, HandFill } from "../glyphs";
import { displayName, FINISHED_TINT, LIMITED_TINT, providerTint, WAITING_TINT } from "../format";
import { SessionAvatar } from "../SessionAvatar";
import { S } from "../strings";
import { useSessions } from "../store";
import type { Session } from "../types";

const ERROR_TINT = "#FF5F57";
const OUTCOME: Record<Outcome, { tint: string; Icon: (p: { size: number }) => ReactElement }> = {
  waiting: { tint: WAITING_TINT, Icon: (p) => <HandFill size={p.size} /> },
  error: { tint: ERROR_TINT, Icon: (p) => <CircleX size={p.size} strokeWidth={2.6} /> },
  limited: { tint: LIMITED_TINT, Icon: (p) => <GaugeFill size={p.size} /> },
  finished: { tint: FINISHED_TINT, Icon: (p) => <CheckCircleFill size={p.size} /> },
  working: { tint: "rgba(255,255,255,.7)", Icon: () => <span className="p-away-working" /> },
};

const outcomeLabel = (o: Outcome) =>
  o === "waiting"
    ? S.awayWaiting
    : o === "error"
      ? S.awayError
      : o === "limited"
        ? S.awayLimited
        : o === "finished"
          ? S.awayFinished
          : S.awayWorking;

/** Seconds the card stays before it closes itself, unless the pointer rests on it. */
export const CLOSE_AFTER = 15;

const useClock = create<{ left: number; held: boolean }>(() => ({ left: CLOSE_AFTER, held: false }));

/** Shows the card: the island opens on it, and closes again with it. */
export function presentAway(lines: AwayLine[], since: number) {
  useClock.setState({ left: CLOSE_AFTER, held: false });
  useSessions.setState({ away: { lines, since }, focused: null, composing: false, wardrobe: false });
  useIsland.getState().open("sessions", { takeFocus: false });
}

export function closeAway() {
  if (!useSessions.getState().away) return;
  useSessions.setState({ away: null });
  useIsland.getState().collapse();
}

export function AwayPage() {
  useLocale();
  const away = useSessions((s) => s.away);
  const sessions = useSessions((s) => s.snapshot.sessions);
  const names = useSessions((s) => s.names);
  const held = useClock((s) => s.held);
  useEffect(() => {
    if (held) return;
    const id = setInterval(() => {
      const left = Math.max(0, useClock.getState().left - 0.1);
      useClock.setState({ left });
      if (left <= 0) closeAway();
    }, 100);
    return () => clearInterval(id);
  }, [held]);
  if (!away) return null;
  const tiles = away.lines
    .map((line) => ({ line, session: sessions.find((s) => s.id === line.id) }))
    .filter((t): t is { line: AwayLine; session: Session } => !!t.session);
  return (
    <div
      className="p-away-tiles"
      role="list"
      aria-label={S.awayTitle}
      onPointerEnter={() => useClock.setState({ held: true })}
      onPointerLeave={() => useClock.setState({ held: false })}
      onFocus={() => useClock.setState({ held: true })}
    >
      {tiles.map(({ line, session }, i) => (
        <Tile key={line.id} line={line} session={session} name={displayName(session, names[session.id])} i={i} />
      ))}
    </div>
  );
}

function Tile({ line, session, name, i }: { line: AwayLine; session: Session; name: string; i: number }) {
  const reduce = useReducedMotion();
  const { tint, Icon } = OUTCOME[line.outcome];
  const now = Date.now() / 1000;
  return (
    <motion.button
      type="button"
      role="listitem"
      className="p-away-tile"
      style={{ "--p-outcome": tint } as CSSProperties}
      onClick={() => useSessions.getState().focus(session.id)}
      title={session.lastText ?? undefined}
      aria-label={`${name}, ${outcomeLabel(line.outcome)}`}
      {...(reduce
        ? {}
        : {
            initial: { opacity: 0, y: 10, scale: 0.96 },
            animate: { opacity: 1, y: 0, scale: 1 },
            transition: { delay: 0.08 + i * 0.05, type: "spring" as const, stiffness: 380, damping: 28 },
          })}
    >
      <span className="p-away-tile-face">
        <SessionAvatar session={session} now={now} size="page" />
        <span className="p-away-badge is-tile" style={{ color: tint }}>
          <Icon size={12} />
        </span>
      </span>
      <span className="p-away-name">{name}</span>
      <Stats line={line} session={session} />
    </motion.button>
  );
}

function Stats({ line, session }: { line: AwayLine; session: Session }) {
  if (!line.files && !line.tasks) return null;
  return (
    <span className="p-away-stats">
      {line.files > 0 ? (
        <>
          <span className="p-away-files" aria-label={S.awayFiles(line.files)}>
            <FileGlyph />
            {line.files}
          </span>
          {line.added > 0 ? <span className="p-plus">+{line.added}</span> : null}
          {line.removed > 0 ? <span className="p-minus">−{line.removed}</span> : null}
        </>
      ) : null}
      {line.tasks ? <MiniTasks tasks={line.tasks} tint={providerTint(session.provider)} /> : null}
    </span>
  );
}

function FileGlyph() {
  return (
    <svg viewBox="0 0 10 12" width="8" height="10" aria-hidden>
      <path d="M1.5 1h4.5l2.5 2.5V11h-7z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
    </svg>
  );
}

/** Tasks ticked while away: the old share dim, the new part lit. */
function MiniTasks({ tasks, tint }: { tasks: NonNullable<AwayLine["tasks"]>; tint: string }) {
  const bars = Math.min(tasks.total, 12);
  const scale = bars / tasks.total;
  const before = Math.round(tasks.from * scale);
  const after = Math.round(tasks.to * scale);
  return (
    <span
      className="p-away-tasks"
      style={{ "--p-tint": tint } as CSSProperties}
      aria-label={S.tasks(tasks.to, tasks.total)}
    >
      <span className="p-away-tasks-bar" aria-hidden>
        {Array.from({ length: bars }, (_, k) => (
          <span
            key={k}
            className={k < before ? "is-before" : k < after ? "is-new" : undefined}
            style={k >= before && k < after ? { animationDelay: `${0.4 + (k - before) * 0.05}s` } : undefined}
          />
        ))}
      </span>
      {tasks.to}/{tasks.total}
    </span>
  );
}

/** The top row's start: how long the person was gone, and a tally. */
export function AwayLeading() {
  useLocale();
  const away = useSessions((s) => s.away);
  if (!away) return null;
  const minutes = Math.max(1, Math.round((Date.now() / 1000 - away.since) / 60));
  const tally = (["waiting", "error", "limited", "finished"] as Outcome[])
    .map((o) => ({ o, n: away.lines.filter((l) => l.outcome === o).length }))
    .filter((t) => t.n > 0);
  return (
    <span className="p-away-head">
      <MoonStar size={12} strokeWidth={2.2} className="p-away-moon" aria-hidden />
      <span className="p-away-time">{S.awayFor(minutes)}</span>
      <span className="p-away-tally">
        {tally.map(({ o, n }) => {
          const { tint, Icon } = OUTCOME[o];
          return (
            <span key={o} className="p-away-tally-item" style={{ color: tint }} aria-label={`${n} ${outcomeLabel(o)}`}>
              <Icon size={10} />
              {n}
            </span>
          );
        })}
      </span>
    </span>
  );
}

/** The top row's end: close, its ring running down to the card closing itself. */
export function AwayTrailing() {
  useLocale();
  const left = useClock((s) => s.left);
  return (
    <button type="button" className="p-away-close" aria-label={S.awayClose} title={S.awayClose} onClick={closeAway}>
      <svg viewBox="0 0 22 22" width="22" height="22" aria-hidden>
        <circle cx="11" cy="11" r="9.5" className="p-away-close-track" />
        <circle
          cx="11"
          cy="11"
          r="9.5"
          className="p-away-close-ring"
          pathLength="1"
          strokeDasharray={`${left / CLOSE_AFTER} 1`}
          transform="rotate(-90 11 11)"
        />
        <path d="M8 8l6 6M14 8l-6 6" className="p-away-close-x" />
      </svg>
    </button>
  );
}
