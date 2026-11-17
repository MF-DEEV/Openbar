// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every ask waiting on the person, on top of the Sessions list (#269): how
// many, Allow all for the ones that look safe, and each ask on its own row
// with its own buttons. A new ask joins at the foot
// (NotchPermissionQueueView.swift).

import { useLocale } from "../i18n";
import { ArrowRight, BadgeCheck, Check, Hand, X } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { SessionAvatar } from "./SessionAvatar";
import { displayName } from "./format";
import { PLAN_TOOL } from "./PermissionCard";
import { answersHere, offersAlways, rowLine, type PermissionQueue as Queue, type QueueRow } from "./queue";
import { S } from "./strings";
import { SETTLE_DELAY, useSessions } from "./store";

/** Spring(response 0.32, damping 0.86), as rows join and leave. */
const ROW_SPRING = { type: "spring", stiffness: 385, damping: 33.8 } as const;

export function PermissionQueue({ queue, now }: { queue: Queue; now: number }) {
  useLocale();
  const reduce = useReducedMotion();
  const allowAll = queue.allowAll;
  return (
    <section className="s-queue" aria-label={S.queueTitle(queue.count)}>
      <header className="s-queue-head">
        <Hand size={12} strokeWidth={2.4} aria-hidden />
        <span className="s-queue-title">{S.queueTitle(queue.count)}</span>
        {allowAll.length > 0 ? (
          <button
            type="button"
            className="s-queue-all"
            title={S.allowAllHelp}
            aria-label={`${S.allowAll}, ${allowAll.length}`}
            onClick={() => void useSessions.getState().allowAll(queue)}
          >
            <Check size={11} strokeWidth={3} aria-hidden />
            <span>{S.allowAll}</span>
            <span className="s-queue-count">{allowAll.length}</span>
          </button>
        ) : null}
      </header>
      <div className="s-queue-rows">
        <AnimatePresence initial={false}>
          {queue.rows.map((row) => (
            <motion.div
              key={row.id}
              layout={reduce ? false : "position"}
              transition={ROW_SPRING}
              initial={reduce ? { opacity: 0 } : { opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={reduce ? { opacity: 0 } : { opacity: 0, scale: 0.97 }}
            >
              <QueueRowView row={row} now={now} />
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
      {queue.heldBack > 0 && allowAll.length > 0 ? (
        <p className="s-queue-held">{S.queueHeldBack(queue.heldBack)}</p>
      ) : null}
    </section>
  );
}

/** One ask: whose, which tool, what it would run or touch, and why Allow all leaves it out. */
function QueueRowView({ row, now }: { row: QueueRow; now: number }) {
  useLocale();
  const { session, ask } = row;
  const custom = useSessions((s) => s.names[session.id]);
  const lookedAt = useSessions((s) => s.lookedAt[session.id]);
  const decide = useSessions((s) => s.decide);
  const toggleAutoAllow = useSessions((s) => s.toggleAutoAllow);
  // Its buttons wait a moment after it appears, so a click meant for the
  // row that was there cannot answer it.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    setReady(false);
    const timer = setTimeout(() => setReady(true), SETTLE_DELAY * 1000);
    return () => clearTimeout(timer);
  }, [row.id]);
  const name = displayName(session, custom);
  const title = ask.questions.length ? S.question : ask.tool === PLAN_TOOL ? S.plan : ask.tool;
  const risk = S.risk(ask.risk);
  const line = rowLine(ask);
  const prose = ask.tool === PLAN_TOOL || ask.questions.length > 0;
  const shown = { ...session, permission: ask };
  const open = () => useSessions.getState().focus(session.id, ask.requestId);
  return (
    <div className="s-queue-row">
      <button
        type="button"
        className="s-queue-open"
        title={S.queueOpenHelp}
        aria-label={[name, title, risk, line].filter(Boolean).join(", ")}
        onClick={open}
      >
        <SessionAvatar session={session} now={now} lookedAt={lookedAt} size="queue" agentMark />
        <span className="s-queue-text">
          <span className="s-queue-line1">
            <span className="s-queue-name">{name}</span>
            <span className="s-queue-tool">{title}</span>
            {risk ? <span className="s-queue-risk">{risk}</span> : null}
          </span>
          {line ? <span className={`s-queue-detail${prose ? " is-prose" : ""}`}>{line}</span> : null}
        </span>
      </button>
      <div className="s-quick" onClick={(e) => e.stopPropagation()}>
        {answersHere(ask) ? (
          <>
            <button
              type="button"
              className="s-quick-button is-deny"
              aria-label={S.deny}
              title={S.deny}
              disabled={!ready}
              onClick={() => void decide(shown, "deny")}
            >
              <X size={12} strokeWidth={3} />
            </button>
            {offersAlways(row) ? (
              <button
                type="button"
                className="s-quick-button is-secondary"
                aria-label={S.alwaysAllow}
                title={S.alwaysAllow}
                disabled={!ready}
                onClick={() => {
                  // Codex's Always lasts its turn, as on its card.
                  if (session.provider === "codex") {
                    if (!useSessions.getState().autoAllow[session.id]) toggleAutoAllow(session.id);
                    void decide(shown, "allow");
                  } else void decide(shown, "always");
                }}
              >
                <BadgeCheck size={12} strokeWidth={2.4} />
              </button>
            ) : null}
            <button
              type="button"
              className="s-quick-button is-allow"
              aria-label={S.allow}
              title={S.allow}
              disabled={!ready}
              onClick={() => void decide(shown, "allow")}
            >
              <Check size={12} strokeWidth={3} />
            </button>
          </>
        ) : (
          // A question, a plan or a command to read whole: on its card.
          <button type="button" className="s-button is-secondary" disabled={!ready} onClick={open}>
            <span>{S.queueReview}</span>
            <ArrowRight size={11} strokeWidth={2.5} />
          </button>
        )}
      </div>
    </div>
  );
}
