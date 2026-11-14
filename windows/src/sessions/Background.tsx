// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Whole sessions running in the background, with no terminal
// (NotchClaudeBackgroundViews.swift): their mark, what can be done with them,
// their recent output, and the page that starts a new session.

import { useSettings } from "../settings/state";
import { HOOK_AGENTS, isHookAgent, PROVIDER_NAMES, PROVIDERS, SHORT_NAMES } from "./types";
import {
  Check,
  ChevronsUpDown,
  Copy,
  Cpu,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Square,
  Trash2,
  UserRound,
} from "lucide-react";
import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from "react";
import { t, tf, useLocale } from "../i18n";
import { useIsland } from "../island/store";
import { bridge, inTauri } from "../shared/bridge";
import { canRemoveBackground, canStopBackground, problemText } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import { ProviderMark } from "./glyphs";
import type {
  ClaudeAccount,
  NewSessionModel,
  NewSessionModels,
  Provider,
  RecentFolder,
  RepoState,
  Session,
  WorktreeRemoval,
} from "./types";
import { isOwnWorktree, worktreeName, worktreeSlug } from "./worktree";

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
const REMEMBER = {
  agent: "openbar.newSession.agent",
  claude: "openbar.newSession.claudeModel",
  codex: "openbar.newSession.codexModel",
  /** The agents that start without a worktree of their own, comma-separated. */
  noWorktree: "openbar.newSession.noWorktree",
  /** The Claude account (config folder) the last session started with (#216). */
  account: "openbar.newSession.claudeAccount",
} as const;

/** Accounts for designing the picker in a plain browser (?sessions&accounts). */
const DEMO_ACCOUNTS: ClaudeAccount[] = [
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
function AccountMenu({
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

function remembered(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private storage off: the choice lasts this visit.
  }
}

/** A model's key in the menu; the default has none. */
const modelKey = (model: NewSessionModel) => model.id ?? "";

/** The model's name; for the default, which model that is, when known. */
function modelLabel(model: NewSessionModel | undefined): string {
  if (!model) return S.defaultModel;
  if (model.id === null) return model.detail ?? S.defaultModel;
  return model.name;
}

function menuText(model: NewSessionModel): string {
  if (model.id !== null) return model.name;
  return model.detail ? `${S.defaultModel} · ${model.detail}` : S.defaultModel;
}

const AGENT_PREF = {
  gemini: "agentGemini",
  copilot: "agentCopilot",
  opencode: "agentOpencode",
  cursor: "agentCursor",
  pi: "agentPi",
  vibe: "agentVibe",
  devin: "agentDevin",
} as const;

/** The agents this PC can start, the pill sliding to the one picked. */
function AgentPicker({
  agents,
  agent,
  onPick,
}: {
  agents: Provider[];
  agent: Provider;
  onPick: (p: Provider) => void;
}) {
  const index = Math.max(0, agents.indexOf(agent));
  // Past three agents only marks show; the picked one is named beside them.
  const compact = agents.length > 3;
  return (
    <div className="s-agent-picker-wrap">
      <div
        className={`s-agent-picker${compact ? " is-compact" : ""}`}
        role="radiogroup"
        aria-label={S.agentPicker}
        style={{ "--count": agents.length, "--index": index } as CSSProperties}
      >
        <span className="s-agent-pill" aria-hidden />
        {agents.map((provider) => {
          const selected = provider === agent;
          return (
            <button
              key={provider}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={PROVIDER_NAMES[provider]}
              title={PROVIDER_NAMES[provider]}
              className={`s-agent-option${selected ? " is-selected" : ""}`}
              onClick={() => onPick(provider)}
              onKeyDown={(event) => {
                if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
                event.preventDefault();
                const step = event.key === "ArrowRight" ? 1 : -1;
                onPick(agents[(index + step + agents.length) % agents.length]);
              }}
            >
              <ProviderMark provider={provider} size={provider === "claude" ? 12 : 11} />
              {compact ? null : <span>{SHORT_NAMES[provider]}</span>}
            </button>
          );
        })}
      </div>
      {compact ? (
        <span key={agent} className="s-agent-picked">
          {SHORT_NAMES[agent]}
        </span>
      ) : null}
    </div>
  );
}

/** The model, picked from a menu that opens upward: Codex lists more than fit as chips. */
function ModelMenu({
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

/** Starts a session: which folder, which agent and model, what to say first,
 * background or terminal. */
export function NewSession() {
  useLocale();
  const backgroundOn = useSessions((s) => s.snapshot.backgroundAvailable === true);
  const canClaude = useSessions((s) => s.snapshot.canStart === true);
  const canCodex = useSessions((s) => s.snapshot.canStartCodex === true);
  const startable = useSessions((s) => s.snapshot.startableAgents);
  const sessionPrefs = useSettings((s) => s.prefs.sessions);
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [chosen, setChosen] = useState<RecentFolder | null>(null);
  const [prompt, setPrompt] = useState("");
  const [inBackground, setInBackground] = useState(true);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [models, setModels] = useState<NewSessionModels>({ claude: [], codex: [] });
  const [picked, setPicked] = useState<Provider>(() => {
    const saved = remembered(REMEMBER.agent);
    return PROVIDERS.find((p) => p === saved) ?? "claude";
  });
  const [claudeModel, setClaudeModel] = useState(() => remembered(REMEMBER.claude));
  const [codexModel, setCodexModel] = useState(() => remembered(REMEMBER.codex));
  const [noWorktree, setNoWorktree] = useState(() => remembered(REMEMBER.noWorktree));
  const [accounts, setAccounts] = useState<ClaudeAccount[]>([]);
  const [accountChoice, setAccountChoice] = useState<string | null>(null);
  const [lastAccount, setLastAccount] = useState(() => remembered(REMEMBER.account));
  const [worktreeNameText, setWorktreeNameText] = useState("");
  /** What the folder picked is to git; null while it's being asked. */
  const [repository, setRepository] = useState<RepoState | null>(null);

  // Claude Code, Codex, and the hook agents found on this PC and shown in Sessions.
  const agents: Provider[] = [
    ...(canClaude ? (["claude"] as const) : []),
    ...(canCodex ? (["codex"] as const) : []),
    ...HOOK_AGENTS.filter((a) => startable?.includes(a) && sessionPrefs[AGENT_PREF[a]] !== false),
  ];
  const agent: Provider = agents.includes(picked) ? picked : (agents[0] ?? "claude");
  // Codex runs threads in its own background server, so it needs no Agent
  // View; the agents after it run only in a console.
  const canRunInBackground = agent === "codex" || (agent === "claude" && backgroundOn);
  const background = inBackground && canRunInBackground;
  const list = agent === "codex" ? models.codex : agent === "claude" ? models.claude : [];
  const key = agent === "codex" ? codexModel : claudeModel;
  const model = list.find((m) => modelKey(m) === key) ?? list[0];
  // A session in the background has nobody to type to it, so it starts with a prompt.
  const needsPrompt = background && prompt.trim() === "";
  const promptPlaceholder = background ? S.firstPromptRequired : S.firstPrompt;
  const canStart = chosen !== null && !starting && !needsPrompt;
  const message = problem ?? (needsPrompt ? S.backgroundNeedsPrompt : null);
  // A worktree of its own for each agent, as last chosen for it; on until turned off.
  const wantsWorktree = !noWorktree.split(",").includes(agent);
  const isRepository = repository !== null && repository.kind !== "notRepo";
  const usesWorktree = wantsWorktree && isRepository;
  const worktreePlaceholder = worktreeSlug(prompt) || S.worktreeNamePlaceholder;
  const worktreeNote =
    repository?.kind === "notRepo"
      ? S.worktreeNotRepo
      : repository?.kind === "dirty" && wantsWorktree
        ? S.worktreeFromHead
        : null;
  const folderList = useRef<HTMLDivElement>(null);
  const [highlight, setHighlight] = useState<{ top: number; height: number } | null>(null);

  // One highlight, sliding to the folder picked.
  useLayoutEffect(() => {
    const row = folderList.current?.querySelector<HTMLElement>(".s-new-folder.is-selected");
    setHighlight(row ? { top: row.offsetTop, height: row.offsetHeight } : null);
  }, [chosen, folders]);

  useEffect(() => {
    void bridge.sessions.recentFolders().then((found) => {
      setFolders(found ?? []);
      setChosen((current) => current ?? found?.[0] ?? null);
    });
  }, []);

  useEffect(() => {
    const path = chosen?.path;
    setRepository(null);
    if (!path) return;
    let gone = false;
    // An answer for a folder no longer picked is old news.
    void bridge.sessions.repoState(path).then((state) => !gone && setRepository(state ?? { kind: "notRepo" }));
    return () => {
      gone = true;
    };
  }, [chosen?.path]);

  const toggleWorktree = () => {
    const off = new Set(noWorktree.split(",").filter(Boolean));
    if (wantsWorktree) off.add(agent);
    else off.delete(agent);
    const value = [...off].sort().join(",");
    setNoWorktree(value);
    remember(REMEMBER.noWorktree, value);
  };

  useEffect(() => {
    const demo = !inTauri && new URLSearchParams(location.search).has("accounts");
    void bridge.sessions.accounts().then((found) => setAccounts(found ?? (demo ? DEMO_ACCOUNTS : [])));
  }, []);

  // A choice while more than one account is shown; else the folder's own, as before (#216).
  const shownAccounts = accounts.filter((a) => !a.hidden);
  const picksAccount = agent === "claude" && shownAccounts.length > 1;
  const account =
    shownAccounts.find((a) => a.id === accountChoice) ?? preferredAccount(accounts, lastAccount, chosen?.configHome);
  const configHome = shownAccounts.length > 1 ? (account?.path ?? chosen?.configHome) : chosen?.configHome;

  useEffect(() => {
    void bridge.sessions.newModels(configHome).then((found) => {
      if (found) setModels(found);
    });
  }, [configHome]);

  // Escape steps back to the list before it reaches the island.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // An open model menu takes Escape for itself.
      if (document.querySelector(".s-model-list.is-open")) return;
      event.stopImmediatePropagation();
      useSessions.setState({ composing: false });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const pickAgent = (provider: Provider) => {
    if (provider === agent) return;
    setProblem(null);
    setPicked(provider);
    remember(REMEMBER.agent, provider);
  };

  const pickModel = (value: string) => {
    if (agent === "codex") {
      setCodexModel(value);
      remember(REMEMBER.codex, value);
    } else {
      setClaudeModel(value);
      remember(REMEMBER.claude, value);
    }
  };

  const browse = async () => {
    const path = await bridge.sessions.pickFolder();
    if (!path) return;
    const existing = folders.find((folder) => folder.path === path);
    const folder = existing ?? {
      path,
      name: path.split(/[\\/]/).filter(Boolean).pop() ?? path,
      configHome: folders[0]?.configHome,
    };
    if (!existing) setFolders([folder, ...folders]);
    setChosen(folder);
    setProblem(null);
  };

  const start = async () => {
    if (!chosen || !canStart) return;
    setStarting(true);
    setProblem(null);
    if (picksAccount && account) {
      setLastAccount(account.path);
      remember(REMEMBER.account, account.path);
    }
    const failure = await bridge.sessions.start(
      chosen.path,
      agent === "claude" ? configHome : chosen.configHome,
      prompt.trim(),
      background,
      agent,
      model?.id ?? null,
      usesWorktree,
      worktreeNameText.trim() || null,
    );
    setStarting(false);
    if (failure === "unavailable" && agent === "codex") setProblem(S.codexFailed);
    else if (failure) setProblem(problemText(failure));
    else if (failure === null) useSessions.setState({ composing: false });
    else setProblem(agent === "codex" ? S.codexFailed : S.commandFailed);
  };

  return (
    <div className="s-new-session" data-agent={agent}>
      <div ref={folderList} className="s-new-folders" role="listbox" aria-label={S.newSession}>
        {highlight ? (
          <span
            className="s-new-highlight"
            aria-hidden
            style={{ transform: `translateY(${highlight.top}px)`, height: highlight.height }}
          />
        ) : null}
        {folders.length === 0 ? <span className="s-new-none">{S.noRecentFolders}</span> : null}
        {folders.map((folder) => {
          const selected = chosen?.path === folder.path;
          return (
            <button
              key={folder.path}
              type="button"
              role="option"
              aria-selected={selected}
              aria-label={folder.path}
              className={`s-new-folder${selected ? " is-selected" : ""}`}
              onClick={() => {
                setChosen(folder);
                setProblem(null);
              }}
            >
              <span key={selected ? "picked" : "folder"} className={`s-new-icon${selected ? " is-picked" : ""}`}>
                {selected ? <Check size={12} className="s-new-check" /> : <Folder size={12} />}
              </span>
              <span className="s-new-name">{folder.name}</span>
              <span className="s-new-path">{folder.path}</span>
            </button>
          );
        })}
        <button type="button" className="s-new-folder" onClick={() => void browse()}>
          <FolderPlus size={12} />
          <span className="s-new-name">{S.browseFolder}</span>
        </button>
      </div>
      <input
        className="s-new-prompt"
        value={prompt}
        placeholder={promptPlaceholder}
        aria-label={promptPlaceholder}
        onChange={(event) => setPrompt(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void start()}
      />
      {message ? (
        <span
          key={message}
          className={`s-new-message${problem ? " s-background-problem" : ""}`}
          role={problem ? "alert" : "status"}
        >
          {message}
        </span>
      ) : null}
      <div className="s-worktree-row">
        <div className="s-background-row">
          <button
            type="button"
            role="switch"
            aria-checked={usesWorktree}
            disabled={!isRepository}
            className={`s-background-button s-worktree-toggle${usesWorktree ? " is-selected" : ""}`}
            title={`${S.ownWorktreeHelp} ${S.worktreeIgnoredFiles}`}
            onClick={toggleWorktree}
          >
            <span className={`s-worktree-box${usesWorktree ? " is-on" : ""}`} aria-hidden>
              {usesWorktree ? <Check size={9} strokeWidth={3} /> : null}
            </span>
            <GitBranch size={11} />
            {S.ownWorktree}
          </button>
          {usesWorktree ? (
            <input
              className="s-worktree-input"
              value={worktreeNameText}
              placeholder={worktreePlaceholder}
              aria-label={S.worktreeNamePlaceholder}
              spellCheck={false}
              onChange={(event) => setWorktreeNameText(event.target.value)}
              onKeyDown={(event) => event.key === "Enter" && void start()}
            />
          ) : null}
        </div>
        {worktreeNote ? (
          <span key={worktreeNote} className="s-new-message" role="status">
            {worktreeNote}
          </span>
        ) : null}
      </div>
      {agents.length > 1 || list.length > 0 || picksAccount ? (
        <div className="s-background-row">
          {agents.length > 1 ? <AgentPicker agents={agents} agent={agent} onPick={pickAgent} /> : null}
          <span className="s-background-gap" />
          {picksAccount ? (
            <AccountMenu accounts={shownAccounts} value={account} onPick={(a) => setAccountChoice(a.id)} />
          ) : null}
          {list.length > 0 ? <ModelMenu models={list} value={model} agent={agent} onPick={pickModel} /> : null}
        </div>
      ) : null}
      <div className="s-background-row">
        {canRunInBackground ? (
          <>
            <button
              type="button"
              className={`s-background-button${inBackground ? " is-selected" : ""}`}
              aria-pressed={inBackground}
              onClick={() => setInBackground(true)}
            >
              {S.runInBackground}
            </button>
            <button
              type="button"
              className={`s-background-button${inBackground ? "" : " is-selected"}`}
              aria-pressed={!inBackground}
              onClick={() => setInBackground(false)}
            >
              {S.runInTerminal}
            </button>
          </>
        ) : (
          <span className="s-background-off">
            {isHookAgent(agent) ? tf("windows.sessions.opensInTerminal", PROVIDER_NAMES[agent]) : S.backgroundOffHelp}
          </span>
        )}
        <span className="s-background-gap" />
        <button
          type="button"
          className={`s-new-start${starting ? " is-starting" : ""}`}
          disabled={!canStart}
          aria-busy={starting}
          onClick={() => void start()}
        >
          <span className="s-new-start-text">{S.startSession}</span>
          <span className="s-new-spinner" aria-hidden />
        </button>
      </div>
    </div>
  );
}
