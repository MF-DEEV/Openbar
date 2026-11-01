// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Whole sessions running in the background, with no terminal
// (NotchClaudeBackgroundViews.swift): their mark, what can be done with them,
// their recent output, and the page that starts a new session.

import { Check, ChevronsUpDown, Cpu, FileText, Folder, FolderPlus, Square, Trash2 } from "lucide-react";
import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useLocale } from "../i18n";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { canRemoveBackground, canStopBackground, problemText } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import { ProviderMark } from "./glyphs";
import type { NewSessionModel, NewSessionModels, Provider, RecentFolder, Session } from "./types";

/** Says a session runs in the background, in words, on its row. */
export function BackgroundMark() {
  useLocale();
  return (
    <span className="s-background-mark" title={S.backgroundHelp}>
      {S.backgroundMark}
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
  const [asking, setAsking] = useState<"stop" | "remove" | null>(null);
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const run = async (action: "stop" | "remove") => {
    setWorking(true);
    const sessions = bridge.sessions;
    const failure = await (action === "stop"
      ? sessions.stopBackground(session.id)
      : sessions.removeBackground(session.id));
    setWorking(false);
    setAsking(null);
    setProblem(problemText(failure));
  };

  return (
    <div className="s-background-bar">
      {asking ? (
        <div className="s-background-row">
          <span className="s-background-ask">{asking === "stop" ? S.stopConfirm : S.removeConfirm}</span>
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
            {asking === "stop" ? S.stopSession : S.removeSession}
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
  agent: "vorssaint.newSession.agent",
  claude: "vorssaint.newSession.claudeModel",
  codex: "vorssaint.newSession.codexModel",
} as const;

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

/** Claude or Codex, the pill sliding to the one picked. */
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
  return (
    <div
      className="s-agent-picker"
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
            aria-label={provider === "claude" ? S.claudeCode : S.codex}
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
            <span>{provider === "claude" ? "Claude" : "Codex"}</span>
          </button>
        );
      })}
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
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [chosen, setChosen] = useState<RecentFolder | null>(null);
  const [prompt, setPrompt] = useState("");
  const [inBackground, setInBackground] = useState(true);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [models, setModels] = useState<NewSessionModels>({ claude: [], codex: [] });
  const [picked, setPicked] = useState<Provider>(() => (remembered(REMEMBER.agent) === "codex" ? "codex" : "claude"));
  const [claudeModel, setClaudeModel] = useState(() => remembered(REMEMBER.claude));
  const [codexModel, setCodexModel] = useState(() => remembered(REMEMBER.codex));

  const agents: Provider[] = [...(canClaude ? (["claude"] as const) : []), ...(canCodex ? (["codex"] as const) : [])];
  const agent: Provider = agents.includes(picked) ? picked : (agents[0] ?? "claude");
  // Codex runs threads in its own background server, so it needs no Agent View.
  const canRunInBackground = agent === "codex" || backgroundOn;
  const background = inBackground && canRunInBackground;
  const list = agent === "codex" ? models.codex : models.claude;
  const key = agent === "codex" ? codexModel : claudeModel;
  const model = list.find((m) => modelKey(m) === key) ?? list[0];
  // A session in the background has nobody to type to it, so it starts with a prompt.
  const needsPrompt = background && prompt.trim() === "";
  const promptPlaceholder = background ? S.firstPromptRequired : S.firstPrompt;
  const canStart = chosen !== null && !starting && !needsPrompt;
  const message = problem ?? (needsPrompt ? S.backgroundNeedsPrompt : null);
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
    void bridge.sessions.newModels(chosen?.configHome).then((found) => {
      if (found) setModels(found);
    });
  }, [chosen?.configHome]);

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
    const failure = await bridge.sessions.start(
      chosen.path,
      chosen.configHome,
      prompt.trim(),
      background,
      agent,
      model?.id ?? null,
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
      {agents.length > 1 || list.length > 0 ? (
        <div className="s-background-row">
          {agents.length > 1 ? <AgentPicker agents={agents} agent={agent} onPick={pickAgent} /> : null}
          <span className="s-background-gap" />
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
          <span className="s-background-off">{S.backgroundOffHelp}</span>
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
