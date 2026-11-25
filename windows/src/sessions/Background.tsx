// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Whole sessions running in the background, with no terminal
// (NotchClaudeBackgroundViews.swift): their mark, what can be done with them,
// their recent output, and the page that starts a new session.

import {
  Check,
  ChevronsUpDown,
  Copy,
  Cpu,
  FileText,
  FolderOpen,
  GitBranch,
  Square,
  Trash2,
  UserRound,
} from "lucide-react";
import { type CSSProperties, useEffect, useRef, useState } from "react";
import { t, tf, useLocale } from "../i18n";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { canRemoveBackground, canStopBackground, problemText } from "./format";
import { S } from "./strings";
import type { ClaudeAccount, NewSessionModel, Provider, Session, SessionOrigin, WorktreeRemoval } from "./types";
import { isOwnWorktree, worktreeName } from "./worktree";

/** Says a session runs in the background, in words, on its row. */
export function BackgroundMark() {
  useLocale();
  return (
    <span className="s-background-mark" title={S.backgroundHelp}>
      {S.backgroundMark}
    </span>
  );
}

/** The SSH host a session runs on, beside its name, so a remote session
 * never passes for one on this PC (sessions/remote.rs). */
export function RemoteHostMark({ alias }: { alias: string }) {
  useLocale();
  const shown = alias.length > 22 ? alias.slice(0, 21) + "…" : alias;
  return (
    <span className="s-remote-mark" title={S.remoteHelp(alias)} aria-label={S.remoteHelp(alias)}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden>
        <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.6" />
        <path
          d="M1.8 8h12.4M8 1.8c2 2 2 10.4 0 12.4M8 1.8c-2 2-2 10.4 0 12.4"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.3"
        />
      </svg>
      {shown}
    </span>
  );
}

/** The multiplexer a session's pane belongs to, local tmux or zellij (#235). */
export function multiplexerOf(session: Session): "tmux" | "zellij" | null {
  return session.host === "tmux" || session.host === "zellij" ? session.host : null;
}

/** The tmux or zellij a session's pane belongs to, beside its name (#235). */
export function MultiplexerMark({ name, detached = false }: { name: string; detached?: boolean }) {
  useLocale();
  const help = detached ? S.detachedHelp(name) : S.multiplexerHelp(name);
  return (
    <span className="s-multiplexer-mark" title={help} aria-label={help}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden>
        <rect x="1.8" y="2.8" width="12.4" height="10.4" rx="2" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 2.8v10.4" stroke="currentColor" strokeWidth="1.5" />
      </svg>
      {detached ? `${name} · ${S.detachedMark}` : name}
    </span>
  );
}

/** The editor whose own panel runs an agent's session: Cline in VS Code (#241). */
export function EditorMark({ agent, editor }: { agent: string; editor: string }) {
  useLocale();
  const help = S.agentInEditor(agent, editor);
  return (
    <span className="s-multiplexer-mark" title={help} aria-label={help}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden>
        <path d="M5.5 4.5 2 8l3.5 3.5M10.5 4.5 14 8l-3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
      {editor}
    </span>
  );
}

/** The Claude app's Code tab or Cowork, beside the name of a session it runs,
 * so it never passes for one in a terminal (#246). */
export function OriginMark({ origin }: { origin: SessionOrigin }) {
  useLocale();
  const help = S.originHelp(origin);
  return (
    <span className="s-remote-mark s-origin-mark" title={help} aria-label={help}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden>
        {origin === "cowork" ? (
          <path
            d="M1.8 4.2c0-.6.5-1.1 1.1-1.1h3.3l1.5 1.6h5.4c.6 0 1.1.5 1.1 1.1v6.3c0 .6-.5 1.1-1.1 1.1H2.9c-.6 0-1.1-.5-1.1-1.1z"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
          />
        ) : (
          <>
            <rect
              x="1.8"
              y="2.6"
              width="12.4"
              height="10.8"
              rx="2"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.4"
            />
            <path d="M1.8 5.6h12.4" stroke="currentColor" strokeWidth="1.4" />
          </>
        )}
      </svg>
      {S.originMark(origin)}
    </span>
  );
}

/** The Claude account a session runs under, while more than one is in use (#216). */
export function AccountMark({ name }: { name: string }) {
  useLocale();
  const shown = name.length > 16 ? name.slice(0, 15) + "…" : name;
  const label = tf("windows.claudeAccounts.sessionAccount", name);
  return (
    <span className="s-remote-mark s-account-mark" title={label} aria-label={label}>
      <svg viewBox="0 0 16 16" width="9" height="9" aria-hidden>
        <circle cx="8" cy="8" r="6.4" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <circle cx="8" cy="6.6" r="2.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
        <path d="M4.2 12.6c1-1.7 2.3-2.4 3.8-2.4s2.8.7 3.8 2.4" fill="none" stroke="currentColor" strokeWidth="1.4" />
      </svg>
      {shown}
    </span>
  );
}

/** Logs, Stop, Remove. Stopping and removing ask first, in place. */
export function BackgroundBar({
  session,
  showingLogs,
  onLogs,
}: {
  session: Session;
  showingLogs: boolean;
  onLogs: (showing: boolean) => void;
}) {
  useLocale();
  const [asking, setAsking] = useState<"stop" | "remove" | "removeAnyway" | null>(null);
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const run = async (action: "stop" | "remove" | "removeAnyway") => {
    setWorking(true);
    const sessions = bridge.sessions;
    if (action === "stop") {
      const failure = await sessions.stopBackground(session.id);
      setWorking(false);
      setAsking(null);
      setProblem(problemText(failure));
      return;
    }
    const outcome = await sessions.removeBackground(session.id, action === "removeAnyway");
    setWorking(false);
    // Its worktree holds work found nowhere else: ask again, in words that say so.
    if (outcome?.kind === "unsaved") return setAsking("removeAnyway");
    setAsking(null);
    setProblem(outcome?.kind === "done" ? null : problemText(outcome?.kind === "failed" ? outcome.message : undefined));
  };
  const askText = asking === "stop" ? S.stopConfirm : asking === "remove" ? S.removeConfirm : S.worktreeUnsaved;
  const actText = asking === "stop" ? S.stopSession : asking === "remove" ? S.removeSession : S.removeAnyway;

  return (
    <div className="s-background-bar">
      {asking ? (
        <div className="s-background-row">
          <span className="s-background-ask">{askText}</span>
          <button type="button" className="s-background-button" onClick={() => setAsking(null)}>
            {S.keepSession}
          </button>
          <button
            type="button"
            className="s-background-button is-destructive"
            disabled={working}
            onClick={() => void run(asking)}
          >
            {asking === "stop" ? <Square size={10} /> : <Trash2 size={11} />}
            {actText}
          </button>
        </div>
      ) : (
        <div className="s-background-row">
          <BackgroundMark />
          <span className="s-background-gap" />
          <button
            type="button"
            className={`s-background-button${showingLogs ? " is-selected" : ""}`}
            aria-pressed={showingLogs}
            onClick={() => {
              onLogs(!showingLogs);
              // Logs read in the enlarged page.
              if (!showingLogs) useIsland.setState({ sessionsEnlarged: true });
            }}
          >
            <FileText size={11} />
            {S.logs}
          </button>
          {canStopBackground(session) ? (
            <button type="button" className="s-background-button" onClick={() => (setProblem(null), setAsking("stop"))}>
              <Square size={10} />
              {S.stopSession}
            </button>
          ) : null}
          {canRemoveBackground(session) ? (
            <button
              type="button"
              className="s-background-button"
              onClick={() => (setProblem(null), setAsking("remove"))}
            >
              <Trash2 size={11} />
              {S.removeSession}
            </button>
          ) : null}
        </div>
      )}
      {problem ? (
        <span className="s-background-problem" role="alert">
          {problem}
        </span>
      ) : null}
    </div>
  );
}

/** The worktree a session works in: open its folder, copy its branch, and
 * remove one the island made once the session is done with it. Claude
 * Code's own go with `claude rm`. */
export function WorktreeBar({ session }: { session: Session }) {
  useLocale();
  const name = worktreeName(session.cwd);
  const [branch, setBranch] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [asking, setAsking] = useState<"remove" | "unsaved" | null>(null);
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const busy =
    session.phase === "generating" || session.phase === "runningTool" || session.phase === "waitingPermission";
  const canRemove = isOwnWorktree(session.cwd) && !busy && session.phase !== "waitingInput";

  useEffect(() => {
    let gone = false;
    void bridge.sessions.worktreeBranch(session.id).then((found) => !gone && setBranch(found ?? null));
    return () => {
      gone = true;
    };
  }, [session.id, session.cwd]);

  if (!name) return null;

  const remove = async (anyway: boolean) => {
    setWorking(true);
    const outcome: WorktreeRemoval | undefined = await bridge.sessions.removeWorktree(session.id, anyway);
    setWorking(false);
    if (outcome?.kind === "unsaved") return setAsking("unsaved");
    setAsking(null);
    if (outcome?.kind === "failed") setProblem(outcome.message || S.commandFailed);
    else if (!outcome) setProblem(S.commandFailed);
  };

  const copy = async () => {
    if (!branch) return;
    try {
      await navigator.clipboard.writeText(branch);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setProblem(S.commandFailed);
    }
  };

  return (
    <div className="s-background-bar s-worktree-bar">
      {asking ? (
        <div className="s-background-row">
          <span className="s-background-ask">{asking === "remove" ? S.removeWorktreeConfirm : S.worktreeUnsaved}</span>
          <button type="button" className="s-background-button" onClick={() => setAsking(null)}>
            {S.keepSession}
          </button>
          <button
            type="button"
            className="s-background-button is-destructive"
            disabled={working}
            onClick={() => void remove(asking === "unsaved")}
          >
            <Trash2 size={11} />
            {asking === "remove" ? S.removeWorktree : S.removeAnyway}
          </button>
        </div>
      ) : (
        <div className="s-background-row">
          <span className="s-worktree-name" title={S.worktreeMark}>
            <GitBranch size={11} />
            {name}
          </span>
          <span className="s-background-gap" />
          <button
            type="button"
            className="s-background-button"
            onClick={() => void bridge.sessions.openWorktree(session.id)}
          >
            <FolderOpen size={11} />
            {S.openWorktree}
          </button>
          {branch ? (
            <button type="button" className="s-background-button" title={branch} onClick={() => void copy()}>
              {copied ? <Check size={11} /> : <Copy size={11} />}
              {copied ? S.branchCopied : S.copyBranch}
            </button>
          ) : null}
          {canRemove ? (
            <button
              type="button"
              className="s-background-button"
              onClick={() => (setProblem(null), setAsking("remove"))}
            >
              <Trash2 size={11} />
              {S.removeWorktree}
            </button>
          ) : null}
        </div>
      )}
      {problem ? (
        <span className="s-background-problem" role="alert">
          {problem}
        </span>
      ) : null}
    </div>
  );
}

/** A background session's recent output, read again while it shows. */
export function BackgroundLogs({ session }: { session: Session }) {
  useLocale();
  const [logs, setLogs] = useState<string | null | undefined>(undefined);
  const ended = session.background?.ended === true;
  useEffect(() => {
    let gone = false;
    const load = async () => {
      const text = await bridge.sessions.backgroundLogs(session.id);
      if (!gone) setLogs(text ?? null);
    };
    void load();
    const timer = ended ? undefined : setInterval(() => void load(), 5000);
    return () => {
      gone = true;
      if (timer) clearInterval(timer);
    };
  }, [session.id, ended]);
  return (
    <div className="s-background-logs">
      <span className="s-section-title">{S.logsTitle}</span>
      <div className="s-background-screen" ref={(node) => node?.scrollTo({ top: node.scrollHeight })}>
        {logs ? <pre>{logs}</pre> : logs === undefined ? null : <p>{logs === null ? S.logsGone : S.logsEmpty}</p>}
      </div>
    </div>
  );
}

/** The agent and models New session last started with, kept for the next visit. */
export const REMEMBER = {
  agent: "openbar.newSession.agent",
  claude: "openbar.newSession.claudeModel",
  codex: "openbar.newSession.codexModel",
  /** The agents that start without a worktree of their own, comma-separated. */
  noWorktree: "openbar.newSession.noWorktree",
  /** The Claude account (config folder) the last session started with (#216). */
  account: "openbar.newSession.claudeAccount",
} as const;

/** Accounts for designing the picker in a plain browser (?sessions&accounts). */
export const DEMO_ACCOUNTS: ClaudeAccount[] = [
  {
    id: "c:\\users\\vors\\.claude",
    path: "C:\\Users\\vors\\.claude",
    name: "Personal",
    named: true,
    manual: false,
    hidden: false,
    hooks: "installed",
  },
  {
    id: "c:\\users\\vors\\.claude-work",
    path: "C:\\Users\\vors\\.claude-work",
    name: "Work",
    named: false,
    manual: false,
    hidden: false,
    hooks: "installed",
  },
];

/** The account a new session starts with: the last one used while it is
 * still shown, else the folder's own, else the first (ClaudeAccounts.preferred). */
export function preferredAccount(
  accounts: readonly ClaudeAccount[],
  last: string,
  folder: string | undefined,
): ClaudeAccount | null {
  const shown = accounts.filter((a) => !a.hidden);
  const same = (a: ClaudeAccount, path: string) =>
    a.path.toLowerCase().replace(/[\\/]+$/, "") === path.toLowerCase().replace(/[\\/]+$/, "");
  return (
    (last ? shown.find((a) => same(a, last)) : undefined) ??
    (folder ? shown.find((a) => same(a, folder)) : undefined) ??
    shown[0] ??
    null
  );
}

/** Which Claude account a session starts with (#216). */
export function AccountMenu({
  accounts,
  value,
  onPick,
}: {
  accounts: readonly ClaudeAccount[];
  value: ClaudeAccount | null;
  onPick: (account: ClaudeAccount) => void;
}) {
  useLocale();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const label = t("windows.claudeAccounts.account");
  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  }, [open]);
  return (
    <div
      ref={root}
      className="s-model-menu s-account-menu"
      onKeyDown={(event) => {
        if (open && event.key === "Escape") {
          event.stopPropagation();
          event.nativeEvent.stopImmediatePropagation();
          setOpen(false);
        }
      }}
    >
      <button
        type="button"
        className={`s-model-button${open ? " is-open" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${value?.name ?? ""}`}
        title={value ? tf("windows.claudeAccounts.startsWith", value.name) : label}
        onClick={() => setOpen(!open)}
      >
        <UserRound size={11} className="s-model-icon" />
        <span className="s-model-label">
          <span key={value?.id ?? "none"} className="s-model-text">
            {value?.name ?? label}
          </span>
        </span>
        <ChevronsUpDown size={10} className="s-model-chevron" />
      </button>
      <div className={`s-model-list${open ? " is-open" : ""}`} role="listbox" aria-label={label} hidden={!open}>
        {accounts.map((account, index) => {
          const selected = account.id === value?.id;
          return (
            <button
              key={account.id}
              type="button"
              role="option"
              aria-selected={selected}
              className="s-model-option"
              style={{ "--i": index } as CSSProperties}
              onClick={() => {
                onPick(account);
                setOpen(false);
              }}
            >
              <Check size={11} className="s-model-check" style={{ opacity: selected ? 1 : 0 }} />
              <span>{account.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function remembered(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

export function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private storage off: the choice lasts this visit.
  }
}

/** A model's key in the menu; the default has none. */
export const modelKey = (model: NewSessionModel) => model.id ?? "";

/** The model's name; for the default, which model that is, when known. */
export function modelLabel(model: NewSessionModel | undefined): string {
  if (!model) return S.defaultModel;
  if (model.id === null) return model.detail ?? S.defaultModel;
  return model.name;
}

function menuText(model: NewSessionModel): string {
  if (model.id !== null) return model.name;
  return model.detail ? `${S.defaultModel} · ${model.detail}` : S.defaultModel;
}

export const AGENT_PREF = {
  gemini: "agentGemini",
  copilot: "agentCopilot",
  opencode: "agentOpencode",
  cursor: "agentCursor",
  pi: "agentPi",
  vibe: "agentVibe",
  devin: "agentDevin",
  antigravity: "agentAntigravity",
  grok: "agentGrok",
  zcode: "agentZcode",
  hermes: "agentHermes",
  qwen: "agentQwen",
  droid: "agentDroid",
  cline: "agentCline",
} as const;

/** The model, picked from a menu that opens upward: Codex lists more than fit as chips. */
export function ModelMenu({
  models,
  value,
  agent,
  onPick,
}: {
  models: NewSessionModel[];
  value: NewSessionModel | undefined;
  agent: Provider;
  onPick: (key: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const label = modelLabel(value);

  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  }, [open]);

  const show = () => {
    setActive(
      Math.max(
        0,
        models.findIndex((m) => m === value),
      ),
    );
    setOpen(true);
  };
  const pick = (model: NewSessionModel) => {
    onPick(modelKey(model));
    setOpen(false);
  };

  return (
    <div
      ref={root}
      className="s-model-menu"
      onKeyDown={(event) => {
        if (!open) {
          if (event.key === "ArrowUp" || event.key === "ArrowDown") {
            event.preventDefault();
            show();
          }
          return;
        }
        if (event.key === "Escape") {
          // Closes the menu before it can step back to the list.
          event.stopPropagation();
          event.nativeEvent.stopImmediatePropagation();
          setOpen(false);
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const step = event.key === "ArrowDown" ? 1 : -1;
          setActive((current) => (current + step + models.length) % models.length);
        } else if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          if (models[active]) pick(models[active]);
        }
      }}
    >
      <button
        type="button"
        className={`s-model-button${open ? " is-open" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${S.modelPicker}: ${label}`}
        title={S.modelPicker}
        onClick={() => (open ? setOpen(false) : show())}
      >
        <Cpu size={11} className="s-model-icon" />
        <span className="s-model-label">
          <span key={`${agent}-${label}`} className="s-model-text">
            {label}
          </span>
        </span>
        <ChevronsUpDown size={10} className="s-model-chevron" />
      </button>
      <div className={`s-model-list${open ? " is-open" : ""}`} role="listbox" aria-label={S.modelPicker} hidden={!open}>
        {models.map((model, index) => {
          const selected = model === value;
          return (
            <button
              key={modelKey(model)}
              type="button"
              role="option"
              aria-selected={selected}
              tabIndex={-1}
              className={`s-model-option${index === active ? " is-active" : ""}`}
              style={{ "--i": index } as CSSProperties}
              onPointerEnter={() => setActive(index)}
              onClick={() => pick(model)}
            >
              <Check size={11} className="s-model-check" style={{ opacity: selected ? 1 : 0 }} />
              <span>{menuText(model)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
