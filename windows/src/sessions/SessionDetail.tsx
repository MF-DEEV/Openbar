// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// One session's page (NotchClaudeSessionDetailView.swift; spec §4.4): who
// it is and what it does, what it asks, its recent tools, subagents and
// last reply, and a field that types a reply into its terminal.

import { ArrowUp, CircleCheck, CircleDashed, CircleX, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { PermissionCard } from "./PermissionCard";
import { SessionAvatar, SubagentAvatar } from "./SessionAvatar";
import { Wardrobe } from "./clawd/Wardrobe";
import { resolvedCostume } from "./clawd/costume";
import { motionForPhase } from "./clawd/sprite";
import { ThinkingRow } from "./ThinkingRow";
import { clock, compactNumber, cost, displayName, phaseClock, phaseTint, projectColor, totalTokens } from "./format";
import { useNow } from "./useNow";
import { S } from "./strings";
import { useSessions } from "./store";
import { isWaiting, isWorking, type ReplyFailure, type Session } from "./types";

export function SessionDetail({ session }: { session: Session }) {
  const now = useNow();
  const custom = useSessions((s) => s.names[session.id]);
  const lookedAt = useSessions((s) => s.lookedAt[session.id]);
  const previews = useSessions((s) => s.snapshot.previews);
  const name = displayName(session, custom);
  const tint = phaseTint(session.phase, session.provider);
  const time = phaseClock(session, now);
  const color = projectColor(session.project);
  const renaming = useSessions((s) => s.renaming);
  const wardrobe = useSessions((s) => s.wardrobe);
  const buddies = useSessions((s) => s.buddies);
  const kept = useSessions((s) => s.costumes[session.id]);

  // Escape closes the wardrobe before it reaches the island.
  useEffect(() => {
    if (!wardrobe) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopImmediatePropagation();
      useSessions.setState({ wardrobe: false });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [wardrobe]);

  if (wardrobe) {
    return (
      <Wardrobe
        sessionId={session.id}
        costume={resolvedCostume(session.id, kept, null, session.provider === "codex" ? "codex" : "clawd")}
        name={name}
        project={session.project}
        motion={motionForPhase(session.phase)}
        since={session.phaseSince * 1000}
        band={false}
        onBack={() => useSessions.setState({ wardrobe: false })}
        onChange={(_, code) => useSessions.getState().setCostume(session.id, code)}
      />
    );
  }
  const open = () => useSessions.getState().openWindow(session);

  const meta = [
    session.branch ? `⎇ ${session.branch}` : undefined,
    session.modelName ?? session.model,
    totalTokens(session.tokens) > 0 ? compactNumber(totalTokens(session.tokens)) : undefined,
    session.cost > 0 ? cost(session.cost) : undefined,
  ].filter(Boolean);

  return (
    <div className="s-detail-page">
      <div className="s-scroll">
        <div className="s-header">
          <button
            type="button"
            className="s-dress"
            disabled={!buddies}
            aria-label="Change outfit"
            title="Change outfit"
            onClick={() => useSessions.setState({ wardrobe: true })}
          >
            <SessionAvatar session={session} now={now} lookedAt={lookedAt} size="page" />
          </button>
          <div className="s-header-text">
            <div className="s-header-line1">
              <span className="s-phase" style={{ color: tint }}>
                {S[session.phase]}
              </span>
              {time ? <span className="s-clock is-page">{time}</span> : null}
              {session.phase === "runningTool" && session.currentTool ? (
                <span className="s-tool">
                  {session.currentTool.summary
                    ? `${session.currentTool.name}: ${session.currentTool.summary}`
                    : session.currentTool.name}
                </span>
              ) : null}
            </div>
            <div className="s-row-line2">
              <span className="s-tag" style={{ color, background: `color-mix(in srgb, ${color} 16%, transparent)` }}>
                {session.project}
              </span>
              <span className="s-secondary">{meta.join(" · ")}</span>
            </div>
          </div>
        </div>

        {isWaiting(session.phase) ? <PermissionCard session={session} onOpen={() => void open()} /> : null}

        {session.title && session.title !== name ? <div className="s-title">{session.title}</div> : null}

        {session.tasks && session.tasks.total > 0 ? (
          <div className="s-tasks">
            <div>
              <span className="s-tasks-count">{S.tasks(session.tasks.done, session.tasks.total)}</span>
              {session.tasks.current ? <span className="s-secondary"> {session.tasks.current}</span> : null}
            </div>
            <div className="s-progress">
              <div style={{ width: `max(4px, ${(session.tasks.done / session.tasks.total) * 100}%)` }} />
            </div>
          </div>
        ) : null}

        {session.timeline.length > 0 ? <Recent session={session} /> : null}
        {session.subagents.length > 0 ? <Subagents session={session} now={now} /> : null}

        <div className="s-section">
          <div className="s-section-title">{S.lastReply}</div>
          {previews ? (
            session.lastText ? (
              <div className="s-last-reply">{session.lastText}</div>
            ) : null
          ) : (
            <div className="s-tertiary">{session.provider === "codex" ? S.previewsOffCodex : S.previewsOff}</div>
          )}
        </div>
      </div>

      {isWorking(session.phase) ? <ThinkingRow session={session} size={13} now={now} /> : null}
      <ReplyField session={session} />

      {renaming ? <RenameField session={session} name={name} /> : null}
    </div>
  );
}

/** Appears above the page; an empty name gives the session its own back. */
function RenameField({ session, name }: { session: Session; name: string }) {
  const [draft, setDraft] = useState(() => useSessions.getState().names[session.id] ?? "");
  const finish = (save: boolean) => {
    if (save) useSessions.getState().rename(session.id, draft);
    useSessions.setState({ renaming: false });
  };
  return (
    <input
      className="s-rename"
      autoFocus
      value={draft}
      maxLength={60}
      placeholder={session.name || name || S.renamePlaceholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") {
          e.stopPropagation();
          finish(false);
        }
      }}
    />
  );
}

function Recent({ session }: { session: Session }) {
  const strip = useRef<HTMLDivElement>(null);
  const newest = session.timeline.at(-1)?.id;
  useEffect(() => {
    strip.current?.scrollTo({ left: strip.current.scrollWidth, behavior: "smooth" });
  }, [newest]);
  return (
    <div className="s-section">
      <div className="s-section-title">{S.recentTools}</div>
      <div className="s-chips" ref={strip}>
        {session.timeline.map((tool) => {
          const running = tool.finished === undefined;
          const Icon = running ? CircleDashed : tool.failed ? CircleX : CircleCheck;
          return (
            <span key={tool.id} className={`s-tool-chip${running ? " is-running" : ""}`}>
              <Icon
                size={9}
                strokeWidth={3}
                style={{ color: running ? "#D97857" : tool.failed ? "#ff453a" : "rgba(255,255,255,0.45)" }}
              />
              <span className={tool.summary ? "is-mono" : undefined}>
                {tool.name}
                {tool.summary ? ` ${tool.summary}` : ""}
              </span>
            </span>
          );
        })}
      </div>
    </div>
  );
}

function Subagents({ session, now }: { session: Session; now: number }) {
  const running = session.subagents.filter((a) => a.finished === undefined).sort((a, b) => a.started - b.started);
  const done = session.subagents
    .filter((a) => a.finished !== undefined)
    .sort((a, b) => (b.finished ?? 0) - (a.finished ?? 0));
  const all = [...running, ...done];
  const shown = all.slice(0, 4);
  return (
    <div className="s-section">
      <div className="s-section-title">{S.subagents}</div>
      {shown.map((a) => (
        <div key={a.id} className="s-subagent">
          <SubagentAvatar session={session} agent={a} />
          <span className="s-subagent-type" style={{ opacity: a.finished === undefined ? 1 : 0.5 }}>
            {a.type}
          </span>
          {a.description ? <span className="s-secondary s-subagent-description">{a.description}</span> : null}
          <span className="s-clock is-small">{clock((a.finished ?? now) - a.started)}</span>
        </div>
      ))}
      {all.length > 4 ? <div className="s-tertiary">{S.moreSubagents(all.length - 4)}</div> : null}
    </div>
  );
}

function unavailable(session: Session): string | null {
  if (isWaiting(session.phase)) return S.replyWhileWaiting;
  if (session.waitingFor) return S.replyWhileDialog;
  if (!session.acceptsReplies) return S.replyUnavailable;
  return null;
}

function ReplyField({ session }: { session: Session }) {
  const draft = useSessions((s) => s.drafts[session.id] ?? "");
  const [failure, setFailure] = useState<ReplyFailure | null>(null);
  const [sending, setSending] = useState(false);
  const reason = unavailable(session);
  const sendable = !reason && !sending && draft.trim().length > 0;

  const send = async () => {
    if (!sendable) return;
    setSending(true);
    setFailure(await useSessions.getState().reply(session, draft, []));
    setSending(false);
  };

  return (
    <div className="s-reply">
      <div className="s-reply-row">
        <input
          className="s-reply-field"
          value={draft}
          disabled={!!reason}
          placeholder={reason ?? (session.provider === "codex" ? S.replyPlaceholderCodex : S.replyPlaceholder)}
          onChange={(e) => {
            setFailure(null);
            useSessions.getState().setDraft(session.id, e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void send();
            }
          }}
        />
        <button
          type="button"
          className={`s-send${sendable ? " is-ready" : ""}`}
          aria-label={S.send}
          disabled={!sendable}
          onClick={() => void send()}
        >
          <ArrowUp size={13} strokeWidth={3} />
        </button>
      </div>
      {failure ? (
        <div className="s-error">
          <TriangleAlert size={10} />
          {failure === "waiting" ? S.sendWhileWaiting : S.sendFailed}
        </div>
      ) : null}
    </div>
  );
}
