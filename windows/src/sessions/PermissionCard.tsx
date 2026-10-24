// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What a waiting session asks, and the buttons that answer it
// (NotchClaudeSessionDetailView.swift:317-579; spec §4.6, §4.7). The whole
// command shows even with previews off: nobody should allow what they
// cannot read. Every new request keeps its buttons off for 0.8 s so a click
// meant for the previous one cannot answer it.

import { ArrowUp, BadgeCheck, Check, CheckSquare, Circle, CircleDot, FastForward, Square, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { phaseIcon } from "./Avatar";
import { phaseTint } from "./format";
import { S } from "./strings";
import { OWN_ANSWER_LIMIT, SETTLE_DELAY, useSessions } from "./store";
import type { Question, Session } from "./types";

/** True once the current request has been on screen for SETTLE_DELAY. */
function useSettled(requestId: string | undefined): boolean {
  const shownAt = useSessions((s) => (requestId ? s.shownAt[requestId] : undefined));
  const [, force] = useState(0);
  useEffect(() => {
    if (shownAt === undefined) return;
    const wait = shownAt + SETTLE_DELAY - Date.now() / 1000;
    if (wait <= 0) return;
    const t = setTimeout(() => force((n) => n + 1), wait * 1000 + 10);
    return () => clearTimeout(t);
  }, [shownAt]);
  return shownAt !== undefined && Date.now() / 1000 - shownAt >= SETTLE_DELAY;
}

export function PermissionCard({ session, onOpen }: { session: Session; onOpen: () => void }) {
  const p = session.permission;
  const tint = phaseTint(session.phase, session.provider);
  const Icon = phaseIcon(session.phase, session.provider);
  const asks = !!p && p.questions.length > 0;
  const title = asks
    ? session.provider === "codex"
      ? S.codexQuestion
      : S.question
    : // "terminal": Codex waits on a request the island can't answer (an MCP form); the card says so.
      (p?.tool ??
      session.currentTool?.name ??
      (session.waitingFor === "terminal" ? undefined : session.waitingFor) ??
      S[session.phase]);

  return (
    <div
      className="s-card"
      style={{
        background: `color-mix(in srgb, ${tint} 8%, transparent)`,
        borderColor: `color-mix(in srgb, ${tint} 35%, transparent)`,
      }}
    >
      <div className="s-card-title" style={{ color: tint }}>
        <Icon size={11} strokeWidth={2.5} />
        <span>{title}</span>
      </div>
      {!p ? (
        <div className="s-card-terminal">
          <span>{S.answerInTerminal}</span>
          <button type="button" className="s-button is-secondary" onClick={onOpen}>
            {S.open}
          </button>
        </div>
      ) : asks ? (
        <QuestionForm key={p.requestId} session={session} />
      ) : (
        <ToolRequest key={p.requestId} session={session} />
      )}
    </div>
  );
}

function ToolRequest({ session }: { session: Session }) {
  const p = session.permission!;
  const ready = useSettled(p.requestId);
  const decide = useSessions((s) => s.decide);
  return (
    <>
      <pre className="s-detail">{p.detail}</pre>
      <div className="s-card-buttons">
        <button
          type="button"
          className="s-button is-deny"
          disabled={!ready}
          onClick={() => void decide(session, "deny")}
        >
          <X size={10} strokeWidth={3.5} />
          {S.deny}
        </button>
        <span className="s-spacer" />
        {p.offersAlways ? (
          <button
            type="button"
            className="s-button is-secondary"
            disabled={!ready}
            onClick={() => void decide(session, "always")}
          >
            <BadgeCheck size={10} strokeWidth={3} />
            {S.alwaysAllow}
          </button>
        ) : null}
        <button
          type="button"
          className="s-button is-allow"
          disabled={!ready}
          onClick={() => void decide(session, "allow")}
        >
          <Check size={10} strokeWidth={3.5} />
          {S.allow}
        </button>
      </div>
      <div className="s-hint">{S.shortcutsHint}</div>
    </>
  );
}

type Picks = Record<number, string[]>;

/** The answers map in the order Claude wrote the options; own text after any picks. */
export function buildAnswers(questions: Question[], picks: Picks, own: Record<number, string>) {
  const answers: Record<string, string | string[]> = {};
  questions.forEach((q, i) => {
    const chosen = q.options.map((o) => o.label).filter((l) => (picks[i] ?? []).includes(l));
    const typed = (own[i] ?? "").trim().slice(0, OWN_ANSWER_LIMIT);
    const all = typed ? [...chosen, typed] : chosen;
    if (all.length === 0) return;
    answers[q.question] = q.multiSelect ? all : all[0]!;
  });
  return answers;
}

function QuestionForm({ session }: { session: Session }) {
  const p = session.permission!;
  const questions = p.questions;
  const ready = useSettled(p.requestId);
  const [picks, setPicks] = useState<Picks>({});
  const [own, setOwn] = useState<Record<number, string>>({});
  const [sending, setSending] = useState(false);
  const answers = useMemo(() => buildAnswers(questions, picks, own), [questions, picks, own]);
  const complete = Object.keys(answers).length === questions.length;
  const answerOnClick =
    questions.length === 1 && !questions[0]!.multiSelect && questions[0]!.options.every((o) => !o.preview);

  const send = async (map = answers) => {
    setSending(true);
    await useSessions.getState().answer(session, map);
    // Still shown two seconds later: it was answered in the terminal meanwhile.
    setTimeout(() => setSending(false), 2000);
  };
  const disabled = !ready || sending;

  const pick = (qi: number, label: string) => {
    const q = questions[qi]!;
    if (q.multiSelect) {
      setPicks((cur) => {
        const list = cur[qi] ?? [];
        return { ...cur, [qi]: list.includes(label) ? list.filter((l) => l !== label) : [...list, label] };
      });
      return;
    }
    setPicks((cur) => ({ ...cur, [qi]: [label] }));
    setOwn((cur) => ({ ...cur, [qi]: "" }));
    if (answerOnClick) void send(buildAnswers(questions, { [qi]: [label] }, {}));
  };

  return (
    <div className="s-questions">
      {questions.map((q, qi) => {
        const picked = picks[qi] ?? [];
        const preview = q.options.find((o) => picked.includes(o.label) && o.preview)?.preview;
        return (
          <div key={qi} className="s-question">
            {q.header ? <span className="s-chip">{q.header}</span> : null}
            <div className="s-question-text">{q.question}</div>
            {q.multiSelect ? <div className="s-secondary">{S.pickAny}</div> : null}
            <div className="s-options">
              {q.options.map((o) => {
                const on = picked.includes(o.label);
                const Marker = q.multiSelect ? (on ? CheckSquare : Square) : on ? CircleDot : Circle;
                return (
                  <button
                    key={o.label}
                    type="button"
                    className={`s-option${on ? " is-on" : ""}`}
                    disabled={disabled}
                    aria-pressed={on}
                    onClick={() => pick(qi, o.label)}
                  >
                    <Marker size={14} strokeWidth={2} />
                    <span className="s-option-text">
                      <span className="s-option-label">{o.label}</span>
                      {o.description ? <span className="s-option-description">{o.description}</span> : null}
                    </span>
                  </button>
                );
              })}
            </div>
            {preview ? <pre className="s-detail is-preview">{preview}</pre> : null}
            <input
              className="s-own"
              value={own[qi] ?? ""}
              disabled={disabled}
              placeholder={
                q.options.length
                  ? S.ownAnswer
                  : session.provider === "codex"
                    ? S.replyPlaceholderCodex
                    : S.replyPlaceholder
              }
              maxLength={OWN_ANSWER_LIMIT}
              onChange={(e) => {
                const text = e.target.value;
                setOwn((cur) => ({ ...cur, [qi]: text }));
                if (!q.multiSelect && text) setPicks((cur) => ({ ...cur, [qi]: [] }));
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && complete && !disabled) void send();
              }}
            />
          </div>
        );
      })}
      <div className="s-card-buttons">
        <button
          type="button"
          className="s-button is-deny"
          disabled={disabled}
          onClick={() => void useSessions.getState().skip(session)}
        >
          <FastForward size={10} strokeWidth={3} />
          {S.skip}
        </button>
        <span className="s-spacer" />
        <button
          type="button"
          className="s-button is-allow"
          disabled={disabled || !complete}
          onClick={() => void send()}
        >
          <ArrowUp size={10} strokeWidth={3.5} />
          {S.sendAnswer}
        </button>
      </div>
      <div className="s-hint">{session.provider === "codex" ? S.codexQuestionHint : S.questionHint}</div>
    </div>
  );
}
