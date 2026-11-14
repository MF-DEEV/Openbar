// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What a waiting session asks, and the buttons that answer it
// (NotchClaudeSessionDetailView.swift:317-579; spec §4.6, §4.7). The whole
// command shows even with previews off: nobody should allow what they
// cannot read. Every new request keeps its buttons off for 0.8 s so a click
// meant for the previous one cannot answer it.

import { PROVIDER_NAMES } from "./types";
import { useLocale, t } from "../i18n";

import {
  ArrowUp,
  BadgeCheck,
  Check,
  CheckSquare,
  ChevronUp,
  Circle,
  CircleDot,
  Eye,
  FastForward,
  ShieldAlert,
  Square,
  TextSearch,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState, useRef } from "react";
import { bridge } from "../shared/bridge";
import { phaseIcon } from "./Avatar";
import { phaseTint } from "./format";
import { S } from "./strings";
import { OWN_ANSWER_LIMIT, SETTLE_DELAY, useSessions } from "./store";
import type { CommandInspection, Question, Session } from "./types";
import { handlePermissionKey } from "./permissionKeys";
import { subscribeGlobalPermissionKeys } from "./globalPermissionKeys";
import { AutoAllowButton } from "./AutoAllowButton";
import { FileDiffView } from "./FileDiffView";
import { HandFill } from "./glyphs";

/** The tool Claude Code calls to have its plan approved (ClaudeToolSummary.planTool). */
export const PLAN_TOOL = "ExitPlanMode";

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
  useLocale();
  const p = session.permission;
  const tint = phaseTint(session.phase, session.provider);
  const Icon = phaseIcon(session.phase, session.provider);
  const asks = !!p && p.questions.length > 0;
  const title = asks
    ? session.provider === "claude"
      ? S.question
      : S.codexQuestion.replace("Codex", PROVIDER_NAMES[session.provider])
    : p?.tool === PLAN_TOOL
      ? S.plan
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
        <>
          {/* A file change only the agent can answer still shows what it changes. */}
          {session.waitingDiffs?.length ? <FileDiffView diffs={session.waitingDiffs} /> : null}
          <div className="s-card-terminal">
            <span>{S.answerIn(session.provider)}</span>
            <button type="button" className="s-button is-secondary" onClick={onOpen}>
              {S.open}
            </button>
          </div>
        </>
      ) : asks ? (
        <QuestionForm key={p.requestId} session={session} />
      ) : (
        <ToolRequest key={`${session.id}:${p.requestId}:${p.command?.revision ?? ""}`} session={session} />
      )}
    </div>
  );
}

function ToolRequest({ session }: { session: Session }) {
  useLocale();
  const p = session.permission!;
  const ready = useSettled(p.requestId);
  const decide = useSessions((s) => s.decide);
  const [inspection, setInspection] = useState<CommandInspection | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [rendered, setRendered] = useState(false);
  const mounted = useRef(true);
  const issued = useRef<string | null>(null);
  const viewer = useRef<HTMLPreElement>(null);
  const expandButton = useRef<HTMLButtonElement>(null);
  const command = p.command;
  const restricted = !!command?.requiresInspection;
  const inspected = !!inspection && inspection.revision === command?.revision;
  const canApprove = ready && (!restricted || (command?.available === true && inspected && rendered));
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (issued.current) void bridge.sessions.closeInspection(issued.current);
    };
  }, []);
  useEffect(() => {
    const node = viewer.current;
    if (!inspected || !node || !inspection) return;
    node.focus({ preventScroll: true });
    const check = () =>
      setRendered(node.clientWidth > 0 && node.clientHeight > 0 && node.textContent === inspection.text);
    check();
    const observer = new ResizeObserver(check);
    observer.observe(node);
    return () => observer.disconnect();
  }, [inspected, inspection]);
  const expand = async () => {
    if (!command?.available || loading) return;
    setLoading(true);
    setFailed(false);
    try {
      const result = await bridge.sessions.inspectCommand(session.id, p.requestId, command.revision);
      if (!mounted.current) {
        if (result) void bridge.sessions.closeInspection(result.token);
        return;
      }
      if (
        !result ||
        result.revision !== command.revision ||
        typeof result.text !== "string" ||
        !result.text ||
        typeof result.token !== "string" ||
        !result.token
      ) {
        if (result?.token) void bridge.sessions.closeInspection(result.token);
        setFailed(true);
        return;
      }
      issued.current = result.token;
      setInspection(result);
    } catch {
      if (mounted.current) setFailed(true);
    } finally {
      if (mounted.current) setLoading(false);
    }
  };
  const collapse = () => {
    if (issued.current) void bridge.sessions.closeInspection(issued.current);
    issued.current = null;
    setRendered(false);
    setInspection(null);
    expandButton.current?.focus();
  };
  const approve = (decision: "allow" | "always", mode?: import("./types").PlanMode) =>
    decide(session, decision, mode, inspected ? inspection.token : undefined);
  // A plan is prose, read in the interface's font.
  const plan = p.tool === PLAN_TOOL;
  // As Claude Code's plan dialog: the mode that lets Claude work on its own
  // first, then approving each edit.
  const [firstMode, ...otherModes] = plan ? (p.planModes ?? []) : [];
  useEffect(() => {
    const unsubscribe = subscribeGlobalPermissionKeys(session.id, p.requestId);
    const onKey = (event: KeyboardEvent) => handlePermissionKey(event, session.id, p.requestId);
    window.addEventListener("keydown", onKey);
    return () => {
      unsubscribe();
      window.removeEventListener("keydown", onKey);
    };
  }, [session.id, p.requestId]);
  return (
    <>
      {restricted ? (
        <>
          <div className={`s-command-notice${inspected ? " is-open" : ""}`} role="status">
            {inspected ? <Eye size={12} strokeWidth={2.25} /> : <ShieldAlert size={12} strokeWidth={2.25} />}
            <span>
              {inspected
                ? S.commandOpen
                : !command?.complete
                  ? S.commandIncomplete
                  : !command.available
                    ? S.commandTooLarge
                    : command.truncated
                      ? S.commandCut
                      : S.commandControls}
            </span>
            {command?.available && (
              <button
                ref={expandButton}
                type="button"
                className="s-button is-secondary"
                aria-expanded={inspected}
                disabled={loading}
                onClick={() => (inspected ? collapse() : void expand())}
              >
                {inspected ? <ChevronUp size={10} strokeWidth={3} /> : <TextSearch size={10} strokeWidth={2.75} />}
                {loading ? S.openingCommand : inspected ? S.hideCommand : S.showCommand}
              </button>
            )}
          </div>
          {failed && (
            <span className="s-command-failed" role="alert">
              {S.commandOpenFailed}
            </span>
          )}
        </>
      ) : null}
      {inspected ? (
        <pre
          ref={viewer}
          tabIndex={0}
          role="region"
          aria-label={S.commandOpen}
          className="s-detail s-full-command"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              collapse();
            }
          }}
        >
          {inspection.text}
        </pre>
      ) : p.diffs?.length ? (
        // A file change shows what changes, not just where.
        // Gaps opened for one request start closed on the next.
        <FileDiffView key={p.requestId} diffs={p.diffs} />
      ) : (
        <pre className={plan ? "s-detail is-plan" : "s-detail"}>{p.detail}</pre>
      )}
      <div className="s-card-buttons">
        <button
          type="button"
          className="s-button is-deny"
          aria-keyshortcuts="Control+n"
          title={t("windows.sessions.ctrlN")}
          disabled={!ready}
          onClick={() => void decide(session, "deny")}
        >
          <X size={10} strokeWidth={3.5} />
          {plan ? S.keepPlanning : S.deny}
        </button>
        <span className="s-spacer" />
        {session.provider === "codex" ? (
          restricted ? (
            <button
              type="button"
              className="s-button is-secondary"
              disabled={!canApprove}
              onClick={() =>
                void approve("allow").then((ok) => {
                  if (ok) useSessions.getState().toggleAutoAllow(session.id);
                })
              }
            >
              <BadgeCheck size={10} strokeWidth={3} />
              {S.alwaysAllow}
            </button>
          ) : (
            <AutoAllowButton session={session} disabled={!canApprove} />
          )
        ) : p.offersAlways ? (
          <button
            type="button"
            className="s-button is-secondary"
            aria-keyshortcuts="Control+t"
            title={t("windows.sessions.ctrlT")}
            disabled={!canApprove}
            onClick={() => void approve("always")}
          >
            <BadgeCheck size={10} strokeWidth={3} />
            {S.alwaysAllow}
          </button>
        ) : null}
        {otherModes.reverse().map((mode) => (
          <button
            key={mode}
            type="button"
            className="s-button is-secondary"
            title={S.planModeHelp(mode)}
            disabled={!canApprove}
            onClick={() => void approve("allow", mode)}
          >
            <HandFill size={10} />
            {S.planMode(mode)}
          </button>
        ))}
        <button
          type="button"
          className="s-button is-allow"
          aria-keyshortcuts="Control+y"
          title={
            firstMode ? `${S.planModeHelp(firstMode)} (${t("windows.sessions.ctrlY")})` : t("windows.sessions.ctrlY")
          }
          disabled={!canApprove}
          onClick={() => void approve("allow", firstMode)}
        >
          <Check size={10} strokeWidth={3.5} />
          {firstMode ? S.planMode(firstMode) : plan ? S.approvePlan : S.allow}
        </button>
      </div>
      <div className="s-hint">
        {restricted
          ? command?.available
            ? S.shortcutsHintInspect
            : S.shortcutsHintDenyOnly
          : firstMode
            ? S.shortcutsHintPlan(S.planMode(firstMode))
            : S.shortcutsHint}
      </div>
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
  useLocale();
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
              placeholder={q.options.length ? S.ownAnswer : S.replyPlaceholderFor(session.provider)}
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
