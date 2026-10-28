// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// One session's page (NotchClaudeSessionDetailView.swift; spec §4.4): who
// it is and what it does, what it asks, its recent tools, subagents and
// last reply, and a field that types a reply into its terminal.

import {
  ArrowUp,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleDashed,
  CircleX,
  PanelRight,
  TriangleAlert,
  X,
} from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { PermissionCard } from "./PermissionCard";
import { AutoAllowButton } from "./AutoAllowButton";
import { SessionAvatar, SubagentAvatar } from "./SessionAvatar";
import { Wardrobe } from "./clawd/Wardrobe";
import { resolvedCostume } from "./clawd/costume";
import { motionForPhase } from "./clawd/sprite";
import { ThinkingRow } from "./ThinkingRow";
import {
  clock,
  compactNumber,
  cost,
  displayName,
  phaseClock,
  phaseTint,
  projectColor,
  providerTint,
  totalTokens,
} from "./format";
import { answerMarks, messageTime } from "./conversation";
import { RichText } from "./RichText";
import { MessageImages } from "./MessageImages";
import { useNow } from "./useNow";
import { S } from "./strings";
import { useSessions } from "./store";
import { setPrefs } from "../shared/prefs";
import { bridge } from "../shared/bridge";
import { IMAGE_LIMIT, imageDataUrl, imageFiles, isImagePasteShortcut } from "./paste";
import { isWaiting, isWorking, type ReplyFailure, type Session } from "./types";

function storedNumber(key: string, fallback: number) {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : Number(value);
  } catch {
    return fallback;
  }
}

export function SessionDetail({ session, enlarged = false }: { session: Session; enlarged?: boolean }) {
  const [sidebarHidden, setSidebarHidden] = useState(() => storedNumber("sessions.sidebarHidden", 0) === 1);
  const [layoutHidden, setLayoutHidden] = useState(sidebarHidden);
  const [textSettling, setTextSettling] = useState(false);
  const sidebarFrame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(sidebarFrame.current), []);
  const [textSize, setTextSize] = useState(() =>
    Math.min(24, Math.max(12, storedNumber("sessions.textSize", 15) || 15)),
  );
  const toggleSidebar = () => {
    const hiding = !layoutHidden;
    try {
      localStorage.setItem("sessions.sidebarHidden", hiding ? "1" : "0");
    } catch {
      /* Memory only. */
    }
    cancelAnimationFrame(sidebarFrame.current);
    setLayoutHidden(hiding);
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      setSidebarHidden(hiding);
      setTextSettling(false);
      return;
    }
    // Match macOS: reflow once under dimmed text, then slide the details
    // independently and restore the conversation's opacity.
    setTextSettling(true);
    sidebarFrame.current = requestAnimationFrame(() => {
      sidebarFrame.current = requestAnimationFrame(() => {
        setSidebarHidden(hiding);
        setTextSettling(false);
      });
    });
  };
  const resize = (step: number) =>
    setTextSize((size) => {
      const next = Math.min(24, Math.max(12, size + step));
      try {
        localStorage.setItem("sessions.textSize", String(next));
      } catch {
        /* Memory only. */
      }
      return next;
    });
  useEffect(() => {
    if (!enlarged) return;
    const key = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || !["+", "=", "-"].includes(event.key)) return;
      event.preventDefault();
      resize(event.key === "-" ? -1 : 1);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [enlarged]);
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
  const buddies = useSessions((s) => s.prefs.buddies);
  const kept = useSessions((s) => s.costumes[session.id]);
  const character = session.provider === "codex" ? "codex" : "clawd";

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
        character={character}
        costume={resolvedCostume(session.id, kept, null, character)}
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
    enlarged ? session.cwd : undefined,
  ].filter(Boolean);

  return (
    <div
      className={`s-detail-page${enlarged ? " is-large" : ""}`}
      style={{ "--chat-size": `${textSize}px`, "--agent-tint": providerTint(session.provider) } as CSSProperties}
    >
      <div className={`s-scroll${enlarged ? " is-large" : ""}`}>
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
          {enlarged ? (
            <div className="s-reading-controls">
              <button
                type="button"
                className="n-icon-button"
                aria-label={S.smallerText}
                title={S.smallerText}
                disabled={textSize <= 12}
                onClick={() => resize(-1)}
              >
                <span className="s-small-a">A</span>
              </button>
              <button
                type="button"
                className="n-icon-button"
                aria-label={S.largerText}
                title={S.largerText}
                disabled={textSize >= 24}
                onClick={() => resize(1)}
              >
                A
              </button>
              <button
                type="button"
                className="n-icon-button"
                aria-label={sidebarHidden ? S.showDetails : S.hideDetails}
                title={sidebarHidden ? S.showDetails : S.hideDetails}
                aria-expanded={!sidebarHidden}
                onClick={toggleSidebar}
              >
                <PanelRight size={14} />
              </button>
            </div>
          ) : null}
        </div>

        {session.provider === "codex" &&
        (!isWaiting(session.phase) || !session.permission || session.permission.questions.length > 0) ? (
          <AutoAllowButton session={session} activeOnly />
        ) : null}
        {isWaiting(session.phase) ? <PermissionCard session={session} onOpen={() => void open()} /> : null}

        <div
          className={
            enlarged
              ? `s-large-body${sidebarHidden ? " is-folded" : ""}${layoutHidden ? " is-layout-folded" : ""}${textSettling ? " is-text-settling" : ""}`
              : "s-detail-body"
          }
        >
          {enlarged ? (
            <Conversation session={session} previews={previews} enlarged layoutKey={`${layoutHidden}:${textSize}`} />
          ) : null}
          <div className={enlarged ? "s-sidebar-layer" : "s-details-wrapper"}>
            {enlarged ? (
              <button
                type="button"
                className="s-sidebar-handle"
                onClick={toggleSidebar}
                aria-label={sidebarHidden ? S.showDetails : S.hideDetails}
                aria-expanded={!sidebarHidden}
              >
                <ChevronRight size={10} />
              </button>
            ) : null}
            <aside
              className={enlarged ? "s-details-sidebar" : "s-details-inline"}
              aria-hidden={enlarged && sidebarHidden}
              inert={enlarged && sidebarHidden}
              aria-label={S.details}
            >
              {session.title && previews ? (
                <div className="s-title">
                  <MessageImages
                    text={session.title}
                    sessionId={session.id}
                    images={
                      /\[Image\s*#\d+\]/i.test(session.title)
                        ? session.chat.find(
                            (m) =>
                              m.role === "user" &&
                              m.images?.length &&
                              (m.text.startsWith(session.title!) ||
                                (!m.text.trim() && /^\s*\[Image\s*#\d+\]\s*$/i.test(session.title!))),
                          )?.images
                        : undefined
                    }
                  />
                </div>
              ) : null}

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

              {session.timeline.length > 0 ? (
                <Recent key={`${session.id}:tools`} session={session} enlarged={enlarged} />
              ) : null}
              {session.subagents.length > 0 ? (
                <Subagents key={`${session.id}:agents`} session={session} now={now} enlarged={enlarged} />
              ) : null}
            </aside>
          </div>

          {!enlarged ? (
            previews ? (
              session.lastText ? (
                <div className="s-section">
                  <div className="s-section-title">{S.lastReply}</div>
                  <div className="s-last-reply">
                    <RichText text={session.lastText} />
                  </div>
                </div>
              ) : null
            ) : (
              <Conversation session={session} previews={false} />
            )
          ) : null}
        </div>
      </div>

      {!enlarged && isWorking(session.phase) ? <ThinkingRow session={session} size={13} now={now} /> : null}
      <ReplyField session={session} enlarged={enlarged} />

      {renaming ? <RenameField session={session} name={name} /> : null}
    </div>
  );
}

function Conversation({
  session,
  previews,
  enlarged = false,
  layoutKey,
}: {
  session: Session;
  previews: boolean;
  enlarged?: boolean;
  layoutKey?: string;
}) {
  const marks = answerMarks(session.chat);
  const now = useNow();
  const [enabling, setEnabling] = useState(false);
  const [failed, setFailed] = useState(false);
  const section = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const last = session.chat.at(-1);
  useEffect(() => {
    const scroll = enlarged ? section.current : section.current?.closest(".s-scroll");
    if (!scroll) return;
    const track = () => {
      following.current = scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight < 60;
    };
    scroll.addEventListener("scroll", track);
    return () => scroll.removeEventListener("scroll", track);
  }, [enlarged]);
  useEffect(() => {
    following.current = true;
  }, [session.id, previews]);
  useEffect(() => {
    const scroll = enlarged ? section.current : section.current?.closest(".s-scroll");
    if (scroll && following.current) scroll.scrollTop = scroll.scrollHeight;
  }, [session.id, previews, last?.id, last?.text, enlarged, session.phase, layoutKey]);

  const enable = async () => {
    setEnabling(true);
    setFailed(false);
    try {
      await setPrefs({ sessions: { previews: true } });
    } catch {
      setFailed(true);
    } finally {
      setEnabling(false);
    }
  };
  return (
    <div className={`s-section s-conversation${enlarged ? " is-large" : ""}`} ref={section}>
      {!previews ? (
        <>
          <div className="s-tertiary">{session.provider === "codex" ? S.previewsOffCodex : S.previewsOff}</div>
          <button type="button" className="s-enable-previews" disabled={enabling} onClick={() => void enable()}>
            {S.enablePreviews}
          </button>
          {failed ? <div className="s-error">{S.previewsFailed}</div> : null}
        </>
      ) : session.chat.length ? (
        <div className="s-messages" role="log" aria-label={S.conversation}>
          {session.chat.map((message) => (
            <div
              key={message.id}
              className={`s-message is-${message.role}${marks.answers.has(message.id) ? " is-answer" : ""}${marks.onTheWay.has(message.id) ? " is-on-the-way" : ""}`}
            >
              {marks.answers.has(message.id) ? (
                <div className="s-answer-rule">
                  <span>{S.answer}</span>
                  <time dateTime={new Date(message.at * 1000).toISOString()}>{messageTime(message.at)}</time>
                  <i />
                </div>
              ) : null}
              {message.role === "user" ? (
                <>
                  <div className="s-message-text">
                    <MessageImages text={message.text} images={message.images} sessionId={session.id} />
                  </div>
                  <time className="s-message-time">{messageTime(message.at)}</time>
                </>
              ) : (
                <RichText text={message.text} />
              )}
            </div>
          ))}
        </div>
      ) : session.lastText ? (
        <div className="s-message is-assistant is-answer">
          <div className="s-answer-rule">
            <span>{S.answer}</span>
            <i />
          </div>
          <RichText text={session.lastText} />
        </div>
      ) : (
        <div className="s-tertiary">{S.noMessages}</div>
      )}
      {enlarged && previews && isWorking(session.phase) ? <ThinkingRow session={session} size={15} now={now} /> : null}
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

function Recent({ session, enlarged = false }: { session: Session; enlarged?: boolean }) {
  const [all, setAll] = useState(false);
  const strip = useRef<HTMLDivElement>(null);
  const newest = session.timeline.at(-1)?.id;
  useEffect(() => {
    strip.current?.scrollTo({ left: strip.current.scrollWidth, behavior: "smooth" });
  }, [newest]);
  if (enlarged) {
    const tools = [...session.timeline].reverse();
    return (
      <div className="s-section">
        <div className="s-section-title">{S.recentTools}</div>
        {tools.slice(0, all ? tools.length : 4).map((tool) => {
          const Icon = tool.finished === undefined ? CircleDashed : tool.failed ? CircleX : CircleCheck;
          return (
            <div key={tool.id} className="s-sidebar-tool">
              <Icon
                size={10}
                style={{
                  color:
                    tool.finished === undefined
                      ? providerTint(session.provider)
                      : tool.failed
                        ? "#ff453a"
                        : "rgba(255,255,255,.4)",
                }}
              />
              <strong>{tool.name}</strong>
              <span className="s-sidebar-command" title={tool.summary}>
                {tool.summary}
              </span>
              {tool.finished !== undefined ? <time>{clock(tool.finished - tool.started)}</time> : null}
            </div>
          );
        })}
        {tools.length > 4 ? (
          <button type="button" className="s-fold-list" onClick={() => setAll(!all)} aria-expanded={all}>
            <ChevronDown size={10} className={all ? "is-open" : ""} />
            {all ? S.showLess : S.showAll(tools.length)}
          </button>
        ) : null}
      </div>
    );
  }
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

function Subagents({ session, now, enlarged = false }: { session: Session; now: number; enlarged?: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const running = session.subagents.filter((a) => a.finished === undefined).sort((a, b) => a.started - b.started);
  const done = session.subagents
    .filter((a) => a.finished !== undefined)
    .sort((a, b) => (b.finished ?? 0) - (a.finished ?? 0));
  const all = [...running, ...done];
  const shown = all.slice(0, enlarged && expanded ? all.length : 4);
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
      {all.length > 4 ? (
        enlarged ? (
          <button type="button" className="s-fold-list" onClick={() => setExpanded(!expanded)} aria-expanded={expanded}>
            <ChevronDown size={10} className={expanded ? "is-open" : ""} />
            {expanded ? S.showLess : S.showAll(all.length)}
          </button>
        ) : (
          <div className="s-tertiary">{S.moreSubagents(all.length - 4)}</div>
        )
      ) : null}
    </div>
  );
}

function unavailable(session: Session): string | null {
  if (isWaiting(session.phase)) return S.replyWhileWaiting;
  if (session.waitingFor) return S.replyWhileDialog;
  if (!session.acceptsReplies) return S.replyUnavailable;
  return null;
}

function ReplyField({ session, enlarged = false }: { session: Session; enlarged?: boolean }) {
  const draft = useSessions((s) => s.drafts[session.id] ?? "");
  const images = useSessions((s) => s.attachments[session.id]);
  const [failure, setFailure] = useState<ReplyFailure | null>(null);
  const [pasteFailure, setPasteFailure] = useState<string | null>(null);
  const [pasting, setPasting] = useState(false);
  const pastingNow = useRef(false);
  const [sending, setSending] = useState(false);
  const reason = unavailable(session);
  const sendable = !reason && !sending && !pasting && (draft.trim().length > 0 || !!images?.length);

  const paste = async (files?: File[]) => {
    if (reason || sending || pastingNow.current) return;
    const room = IMAGE_LIMIT - (useSessions.getState().attachments[session.id]?.length ?? 0);
    if (room <= 0) {
      setPasteFailure("Up to 5 images per message");
      return;
    }
    pastingNow.current = true;
    setPasting(true);
    setPasteFailure(null);
    try {
      const attached = files
        ? await Promise.all(
            files.slice(0, room).map(async (file) => {
              const image = await bridge.sessions.attachImage(await imageDataUrl(file));
              if (!image) throw new Error("Cannot attach image");
              return image;
            }),
          )
        : ((await bridge.sessions.clipboardImages()) ?? []).slice(0, room);
      if (!attached.length) {
        setPasteFailure("No image on clipboard");
        return;
      }
      useSessions.getState().addAttachments(session.id, attached);
    } catch (error) {
      setPasteFailure(error instanceof Error ? error.message : String(error));
    } finally {
      pastingNow.current = false;
      setPasting(false);
    }
  };

  const send = async () => {
    if (!sendable || pastingNow.current) return;
    setSending(true);
    try {
      setFailure(
        await useSessions.getState().reply(
          session,
          draft,
          (images ?? []).map((image) => image.path),
        ),
      );
    } catch {
      setFailure("unreachable");
    } finally {
      setSending(false);
    }
  };

  return (
    <div
      className="s-reply"
      onPaste={(event) => {
        const files = imageFiles(event.clipboardData);
        if (!files.length) return; // Plain text keeps the input's normal Ctrl+V behavior.
        event.preventDefault();
        void paste(files);
      }}
      onKeyDown={(event) => {
        if (!isImagePasteShortcut(event.nativeEvent)) return;
        event.preventDefault();
        event.stopPropagation();
        void paste();
      }}
      onDragOver={(event) => {
        if (Array.from(event.dataTransfer.items).some((item) => item.type.startsWith("image/"))) event.preventDefault();
      }}
      onDrop={(event) => {
        const files = imageFiles(event.dataTransfer);
        if (!files.length) return;
        event.preventDefault();
        void paste(files);
      }}
    >
      {images?.length ? (
        <div className="s-reply-images" aria-label="Attached images">
          {images.map((image, index) => (
            <div className="s-reply-image" key={image.path}>
              <img src={image.preview} alt={`Attachment ${index + 1}`} />
              <button
                type="button"
                disabled={sending}
                aria-label={`${S.removeImage} ${index + 1}`}
                title={S.removeImage}
                onClick={() => useSessions.getState().removeAttachment(session.id, image.path)}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      ) : null}
      <div className="s-reply-row">
        {enlarged ? (
          <textarea
            className="s-reply-field s-composer"
            title={S.pasteImages}
            aria-label={session.provider === "codex" ? S.replyPlaceholderCodex : S.replyPlaceholder}
            value={draft}
            disabled={!!reason || sending}
            maxLength={4000}
            placeholder={reason ?? (session.provider === "codex" ? S.replyPlaceholderCodex : S.replyPlaceholder)}
            onChange={(e) => {
              setFailure(null);
              useSessions.getState().setDraft(session.id, e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void send();
              }
            }}
          />
        ) : (
          <input
            className="s-reply-field"
            title={S.pasteImages}
            aria-label={session.provider === "codex" ? S.replyPlaceholderCodex : S.replyPlaceholder}
            value={draft}
            disabled={!!reason || sending}
            maxLength={4000}
            placeholder={reason ?? (session.provider === "codex" ? S.replyPlaceholderCodex : S.replyPlaceholder)}
            onChange={(e) => {
              setFailure(null);
              useSessions.getState().setDraft(session.id, e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void send();
              }
            }}
          />
        )}
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
      {enlarged ? <div className="s-composer-hint">{S.sendShortcut}</div> : null}
      {pasting ? (
        <div className="s-composer-hint" role="status">
          Adding image…
        </div>
      ) : null}
      {pasteFailure ? (
        <div className="s-error" role="status">
          {pasteFailure}
        </div>
      ) : null}
      {failure ? (
        <div className="s-error">
          <TriangleAlert size={10} />
          {failure === "waiting" ? S.sendWhileWaiting : S.sendFailed}
        </div>
      ) : null}
    </div>
  );
}
