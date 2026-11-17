// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Starts a session in four steps (#270; NotchClaudeNewSessionView.swift):
// the project, the agent with its account and model, the options (worktree,
// safe commands, background or console), and the first prompt. The steps sit
// across the top, Start works from any of them, and each project remembers
// what it last started with, so a repeat session is: project, prompt, Start.

import {
  BadgeCheck,
  Check,
  ChevronLeft,
  ChevronRight,
  CornerUpRight,
  Folder,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Hammer,
  MessageSquareText,
  Moon,
  Play,
  Plus,
  ShieldCheck,
  SlidersHorizontal,
  SquareTerminal,
  Terminal,
  WandSparkles,
  X,
} from "lucide-react";
import { type CSSProperties, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { tf, useLocale } from "../i18n";
import { useSettings } from "../settings/state";
import { bridge, inTauri } from "../shared/bridge";
import {
  AccountMenu,
  AGENT_PREF,
  DEMO_ACCOUNTS,
  ModelMenu,
  modelKey,
  modelLabel,
  preferredAccount,
  remember,
  remembered,
  REMEMBER,
} from "./Background";
import { problemText } from "./format";
import { ProviderMark } from "./glyphs";
import {
  cleanedCommand,
  loadChoices,
  type NewSessionChoices,
  offersSafeCommands,
  safeList as listOf,
  saveChoices,
  type SafeEntry,
  type SafeKind,
  shownSuggested,
} from "./newSessionChoices";
import { S } from "./strings";
import { useSessions } from "./store";
import type { ClaudeAccount, NewSessionModels, Provider, RecentFolder, RepoState } from "./types";
import { HOOK_AGENTS, isHookAgent, PROVIDER_NAMES, PROVIDERS, SHORT_NAMES } from "./types";
import { worktreeSlug } from "./worktree";

const STEPS = ["project", "agent", "options", "prompt"] as const;
type Step = (typeof STEPS)[number];

const KIND_ICON: Record<SafeKind, typeof Hammer> = {
  build: Hammer,
  test: BadgeCheck,
  lint: WandSparkles,
  git: GitBranch,
  custom: Terminal,
};

/** Suggestions for designing the page in a plain browser. */
const DEMO_SAFE: SafeEntry[] = [
  { command: "npm run build", kind: "build" },
  { command: "npm run test", kind: "test" },
  { command: "npm run lint", kind: "lint" },
  { command: "git status", kind: "git" },
  { command: "git diff", kind: "git" },
  { command: "git log", kind: "git" },
];

function stepTitle(step: Step): string {
  switch (step) {
    case "project":
      return S.stepProject;
    case "agent":
      return S.stepAgent;
    case "options":
      return S.stepOptions;
    case "prompt":
      return S.stepPrompt;
  }
}

/** A switch the size of the island's others. */
function Switch({
  on,
  label,
  disabled,
  onChange,
}: {
  on: boolean;
  label: string;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={`s-switch${on ? " is-on" : ""}`}
      onClick={() => onChange(!on)}
    >
      <span className="s-switch-knob" aria-hidden />
    </button>
  );
}

/** One option: its mark, its name and a control, with more below when on. */
function Option({
  icon,
  title,
  on,
  help,
  control,
  inline,
  children,
}: {
  icon: ReactNode;
  /** Left out when `inline` takes its place. */
  title?: string;
  on: boolean;
  help?: string;
  control?: ReactNode;
  /** Sits between the name and the control, taking the room left. */
  inline?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="s-new-option" title={help}>
      <div className="s-new-option-head">
        <span key={on ? "on" : "off"} className={`s-new-option-icon${on ? " is-on" : ""}`} aria-hidden>
          {icon}
        </span>
        {title ? <span className="s-new-option-title">{title}</span> : null}
        {inline ?? <span className="s-new-option-gap" />}
        {control}
      </div>
      {children}
    </div>
  );
}

export function NewSession() {
  useLocale();
  const backgroundOn = useSessions((s) => s.snapshot.backgroundAvailable === true);
  const canClaude = useSessions((s) => s.snapshot.canStart === true);
  const canCodex = useSessions((s) => s.snapshot.canStartCodex === true);
  const startable = useSessions((s) => s.snapshot.startableAgents);
  const sessionPrefs = useSettings((s) => s.prefs.sessions);
  const [step, setStep] = useState<Step>("project");
  const [forward, setForward] = useState(true);
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [chosen, setChosen] = useState<RecentFolder | null>(null);
  const [prompt, setPrompt] = useState("");
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [models, setModels] = useState<NewSessionModels>({ claude: [], codex: [] });
  // The last choices of all projects, for a project with none of its own.
  const [lastAgent, setLastAgent] = useState(() => remembered(REMEMBER.agent));
  const [lastModels, setLastModels] = useState(() => ({
    claude: remembered(REMEMBER.claude),
    codex: remembered(REMEMBER.codex),
  }));
  const [noWorktree, setNoWorktree] = useState(() => remembered(REMEMBER.noWorktree));
  const [lastAccount, setLastAccount] = useState(() => remembered(REMEMBER.account));
  // This project's choices.
  const [agentChoice, setAgentChoice] = useState<Provider | null>(null);
  const [modelChoices, setModelChoices] = useState<Partial<Record<Provider, string>>>({});
  const [accountChoice, setAccountChoice] = useState<string | null>(null);
  const [worktreeChoice, setWorktreeChoice] = useState<boolean | null>(null);
  const [inBackground, setInBackground] = useState(true);
  const [safeOn, setSafeOn] = useState(true);
  const [suggested, setSuggested] = useState<SafeEntry[]>([]);
  const [safeOff, setSafeOff] = useState<string[]>([]);
  const [safeCustom, setSafeCustom] = useState<string[]>([]);
  const [adding, setAdding] = useState(false);
  const [newCommand, setNewCommand] = useState("");
  const [refused, setRefused] = useState(false);
  const [accounts, setAccounts] = useState<ClaudeAccount[]>([]);
  const [worktreeNameText, setWorktreeNameText] = useState("");
  /** What the folder picked is to git; null while it's being asked. */
  const [repository, setRepository] = useState<RepoState | null>(null);
  const promptField = useRef<HTMLTextAreaElement>(null);
  const folderList = useRef<HTMLDivElement>(null);
  const [highlight, setHighlight] = useState<{ top: number; height: number } | null>(null);

  // Claude Code, Codex, and the hook agents found on this PC and shown in Sessions.
  const agents: Provider[] = [
    ...(canClaude ? (["claude"] as const) : []),
    ...(canCodex ? (["codex"] as const) : []),
    ...HOOK_AGENTS.filter((a) => startable?.includes(a) && sessionPrefs[AGENT_PREF[a]] !== false),
  ];
  const picked = agentChoice ?? PROVIDERS.find((p) => p === lastAgent) ?? "claude";
  const agent: Provider = agents.includes(picked) ? picked : (agents[0] ?? "claude");
  // Codex runs threads in its own background server, so it needs no Agent
  // View; the agents after it run only in a console.
  const canRunInBackground = agent === "codex" || (agent === "claude" && backgroundOn);
  const background = inBackground && canRunInBackground;
  const list = agent === "codex" ? models.codex : agent === "claude" ? models.claude : [];
  const key = modelChoices[agent] ?? (agent === "codex" ? lastModels.codex : lastModels.claude);
  const model = list.find((m) => modelKey(m) === key) ?? list[0];
  const needsPrompt = background && prompt.trim() === "";
  const canStart = chosen !== null && !starting;
  const wantsWorktree = worktreeChoice ?? !noWorktree.split(",").includes(agent);
  const isRepository = repository !== null && repository.kind !== "notRepo";
  const usesWorktree = wantsWorktree && isRepository;
  const worktreeNote =
    repository?.kind === "notRepo"
      ? S.worktreeNotRepo
      : repository?.kind === "dirty" && wantsWorktree
        ? S.worktreeFromHead
        : null;
  const shownAccounts = accounts.filter((a) => !a.hidden);
  const picksAccount = agent === "claude" && shownAccounts.length > 1;
  const account =
    shownAccounts.find((a) => a.id === accountChoice) ?? preferredAccount(accounts, lastAccount, chosen?.configHome);
  const configHome = shownAccounts.length > 1 ? (account?.path ?? chosen?.configHome) : chosen?.configHome;
  const offersSafe = offersSafeCommands(agent);
  const visibleSuggested = shownSuggested(suggested, repository?.kind !== "notRepo");
  const safe = listOf(safeOn, agent, suggested, safeOff, safeCustom, repository?.kind !== "notRepo");

  useEffect(() => {
    void bridge.sessions.recentFolders().then((found) => {
      setFolders(found ?? []);
      if (found?.[0]) pick(found[0]);
    });
    const demo = !inTauri && new URLSearchParams(location.search).has("accounts");
    void bridge.sessions.accounts().then((found) => setAccounts(found ?? (demo ? DEMO_ACCOUNTS : [])));
  }, []);

  useEffect(() => {
    const path = chosen?.path;
    setRepository(null);
    if (!path) return;
    let gone = false;
    // An answer for a folder no longer picked is old news.
    void bridge.sessions.repoState(path).then((state) => !gone && setRepository(state ?? { kind: "notRepo" }));
    void bridge.sessions.safeCommands(path).then((found) => !gone && setSuggested(found ?? (inTauri ? [] : DEMO_SAFE)));
    return () => {
      gone = true;
    };
  }, [chosen?.path]);

  useEffect(() => {
    void bridge.sessions.newModels(configHome).then((found) => {
      if (found) setModels(found);
    });
  }, [configHome]);

  // One highlight, sliding to the folder picked.
  useLayoutEffect(() => {
    const row = folderList.current?.querySelector<HTMLElement>(".s-new-folder.is-selected");
    setHighlight(row ? { top: row.offsetTop, height: row.offsetHeight } : null);
  }, [chosen, folders, step]);

  // Escape steps back to the list before it reaches the island.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (document.querySelector(".s-model-list.is-open")) return;
      event.stopImmediatePropagation();
      if (adding) {
        setAdding(false);
        setNewCommand("");
        return;
      }
      useSessions.setState({ composing: false });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [adding]);

  useEffect(() => {
    if (step === "prompt") requestAnimationFrame(() => promptField.current?.focus());
  }, [step]);

  /** Picks a project and takes on what it last started with. */
  function pick(folder: RecentFolder) {
    const saved = loadChoices(folder.path);
    setChosen(folder);
    setAgentChoice(saved?.agent ?? null);
    setModelChoices(saved?.models ?? {});
    setAccountChoice(saved?.account ?? null);
    setWorktreeChoice(saved?.worktree ?? null);
    setInBackground(saved?.background ?? true);
    setSafeOn(saved?.safe ?? true);
    setSafeOff(saved?.safeOff ?? []);
    setSafeCustom(saved?.safeCustom ?? []);
    setWorktreeNameText("");
    setProblem(null);
    return saved !== null;
  }

  const go = (target: Step) => {
    if (target === step) return;
    setForward(STEPS.indexOf(target) > STEPS.indexOf(step));
    setAdding(false);
    setStep(target);
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
    go(pick(folder) ? "prompt" : "agent");
  };

  const addCommand = () => {
    const command = cleanedCommand(newCommand);
    if (!command) {
      if (!newCommand.trim()) {
        setAdding(false);
        return;
      }
      setProblem(S.commandRefused);
      setRefused(false);
      requestAnimationFrame(() => setRefused(true));
      return;
    }
    setProblem(null);
    if (suggested.some((e) => e.command === command)) setSafeOff(safeOff.filter((c) => c !== command));
    else if (!safeCustom.includes(command)) setSafeCustom([...safeCustom, command]);
    setNewCommand("");
    setAdding(false);
  };

  const startOrAsk = () => {
    if (!canStart) return;
    if (needsPrompt) {
      setProblem(S.backgroundNeedsPrompt);
      go("prompt");
      return;
    }
    void start();
  };

  const start = async () => {
    if (!chosen) return;
    setStarting(true);
    setProblem(null);
    const modelValue = model ? modelKey(model) : key;
    const choices: NewSessionChoices = {
      agent,
      account: picksAccount ? account?.id : (accountChoice ?? undefined),
      models: { ...modelChoices, [agent]: modelValue },
      worktree: wantsWorktree,
      background: inBackground,
      safe: safeOn,
      safeOff,
      safeCustom,
    };
    const failure = await bridge.sessions.start(
      chosen.path,
      agent === "claude" ? configHome : chosen.configHome,
      prompt.trim(),
      background,
      agent,
      model?.id ?? null,
      usesWorktree,
      worktreeNameText.trim() || null,
      safe,
    );
    setStarting(false);
    if (failure === "unavailable" && agent === "codex") setProblem(S.codexFailed);
    else if (failure) setProblem(problemText(failure));
    else if (failure === null) {
      saveChoices(chosen.path, choices);
      // The last choices of all projects, for one started for the first time.
      remember(REMEMBER.agent, agent);
      setLastAgent(agent);
      if (agent === "claude" || agent === "codex") {
        remember(REMEMBER[agent], modelValue);
        setLastModels({ ...lastModels, [agent]: modelValue });
      }
      if (picksAccount && account) {
        remember(REMEMBER.account, account.path);
        setLastAccount(account.path);
      }
      const off = new Set(noWorktree.split(",").filter(Boolean));
      if (wantsWorktree) off.delete(agent);
      else off.add(agent);
      const value = [...off].sort().join(",");
      remember(REMEMBER.noWorktree, value);
      setNoWorktree(value);
      useSessions.setState({ composing: false });
    } else setProblem(agent === "codex" ? S.codexFailed : S.commandFailed);
  };

  const index = STEPS.indexOf(step);

  const stepBar = (
    <nav className="s-step-bar" aria-label={S.stepsLabel} style={{ "--step": index } as CSSProperties}>
      {STEPS.map((item, i) => {
        const current = item === step;
        const icon =
          item === "project" ? (
            chosen ? (
              <FolderOpen size={14} />
            ) : (
              <Folder size={14} />
            )
          ) : item === "agent" ? (
            <ProviderMark provider={agent} size={agent === "claude" ? 14 : 13} />
          ) : item === "options" ? (
            <SlidersHorizontal size={14} />
          ) : (
            <MessageSquareText size={14} />
          );
        return (
          <span key={item} className="s-step-item">
            <button
              type="button"
              className={`s-step${current ? " is-current" : ""}${i < index ? " is-done" : ""}`}
              aria-current={current ? "step" : undefined}
              aria-label={S.stepLabel(i + 1, STEPS.length, stepTitle(item))}
              title={stepTitle(item)}
              onClick={() => go(item)}
            >
              <span className="s-step-icon">{icon}</span>
              <span>{stepTitle(item)}</span>
              {item === "prompt" && needsPrompt && !current ? <span className="s-step-dot" aria-hidden /> : null}
            </button>
            {i < STEPS.length - 1 ? <span className={`s-step-line${i < index ? " is-done" : ""}`} aria-hidden /> : null}
          </span>
        );
      })}
    </nav>
  );

  const projectPage = (
    <div ref={folderList} className="s-new-folders" role="listbox" aria-label={S.stepProject}>
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
        const known = loadChoices(folder.path) !== null;
        return (
          <button
            key={folder.path}
            type="button"
            role="option"
            aria-selected={selected}
            aria-label={folder.path}
            className={`s-new-folder${selected ? " is-selected" : ""}`}
            onClick={() => go(pick(folder) ? "prompt" : "agent")}
          >
            <span key={selected ? "picked" : "folder"} className={`s-new-icon${selected ? " is-picked" : ""}`}>
              {selected ? <Check size={13} className="s-new-check" /> : <Folder size={13} />}
            </span>
            <span className="s-new-name">{folder.name}</span>
            <span className="s-new-path">{folder.path}</span>
            {known ? (
              <span className="s-new-known" title={S.rememberedProject}>
                <CornerUpRight size={11} />
              </span>
            ) : null}
          </button>
        );
      })}
      <button type="button" className="s-new-folder" onClick={() => void browse()}>
        <FolderPlus size={13} />
        <span className="s-new-name">{S.browseFolder}</span>
      </button>
    </div>
  );

  const agentPage = (
    <div className="s-agent-step">
      <div className="s-agent-tiles" role="radiogroup" aria-label={S.agentPicker}>
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
              className={`s-agent-tile${selected ? " is-selected" : ""}`}
              data-agent={provider}
              onClick={() => {
                setProblem(null);
                setAgentChoice(provider);
              }}
            >
              <ProviderMark provider={provider} size={provider === "claude" ? 26 : 23} />
              <span>{SHORT_NAMES[provider]}</span>
            </button>
          );
        })}
      </div>
      {picksAccount || list.length > 0 ? (
        <div className="s-new-menus">
          {picksAccount ? (
            <AccountMenu accounts={shownAccounts} value={account} onPick={(a) => setAccountChoice(a.id)} />
          ) : null}
          {list.length > 0 ? (
            <ModelMenu
              models={list}
              value={model}
              agent={agent}
              onPick={(value) => setModelChoices({ ...modelChoices, [agent]: value })}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );

  const worktreeOption = (
    <Option
      icon={<GitBranch size={13} />}
      title={usesWorktree ? undefined : S.ownWorktree}
      on={usesWorktree}
      help={`${S.ownWorktreeHelp} ${S.worktreeIgnoredFiles}`}
      control={<Switch on={usesWorktree} label={S.ownWorktree} disabled={!isRepository} onChange={setWorktreeChoice} />}
      inline={
        usesWorktree ? (
          <input
            className="s-worktree-input s-new-option-field"
            value={worktreeNameText}
            placeholder={worktreeSlug(prompt) || S.worktreeNamePlaceholder}
            aria-label={S.worktreeNamePlaceholder}
            spellCheck={false}
            onChange={(event) => setWorktreeNameText(event.target.value)}
          />
        ) : undefined
      }
    >
      {worktreeNote ? <span className="s-new-option-note">{worktreeNote}</span> : null}
    </Option>
  );

  const safeOption = offersSafe ? (
    <Option
      icon={<ShieldCheck size={13} />}
      title={S.safeCommands}
      on={safeOn}
      help={S.safeCommandsHelp}
      control={
        <>
          {safeOn ? (
            <span key={safe.length} className="s-safe-count" aria-label={S.safeCount(safe.length)}>
              {safe.length}
            </span>
          ) : null}
          <Switch on={safeOn} label={S.safeCommands} onChange={setSafeOn} />
        </>
      }
    >
      {safeOn ? (
        <div className="s-safe-chips">
          {visibleSuggested.map((entry) => {
            const on = !safeOff.includes(entry.command);
            const Icon = KIND_ICON[entry.kind];
            return (
              <button
                key={entry.command}
                type="button"
                aria-pressed={on}
                className={`s-safe-chip${on ? " is-on" : ""}`}
                title={S.safeCommandsHelp}
                onClick={() =>
                  setSafeOff(on ? [...safeOff, entry.command] : safeOff.filter((c) => c !== entry.command))
                }
              >
                <Icon key={on ? "on" : "off"} size={10} className="s-safe-icon" />
                <span>{entry.command}</span>
              </button>
            );
          })}
          {safeCustom.map((command) => (
            <button
              key={command}
              type="button"
              className="s-safe-chip is-on is-custom"
              title={S.removeCommand}
              aria-label={`${S.removeCommand} ${command}`}
              onClick={() => setSafeCustom(safeCustom.filter((c) => c !== command))}
            >
              <X size={10} className="s-safe-icon" />
              <span>{command}</span>
            </button>
          ))}
          {adding ? (
            <input
              autoFocus
              className={`s-safe-add-field${refused ? " is-refused" : ""}`}
              value={newCommand}
              placeholder="make check"
              aria-label={S.addCommand}
              spellCheck={false}
              onAnimationEnd={() => setRefused(false)}
              onChange={(event) => {
                setNewCommand(event.target.value);
                setRefused(false);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") addCommand();
              }}
            />
          ) : (
            <button
              type="button"
              className="s-safe-add"
              title={S.addCommand}
              aria-label={S.addCommand}
              onClick={() => setAdding(true)}
            >
              <Plus size={11} />
            </button>
          )}
        </div>
      ) : null}
    </Option>
  ) : null;

  const runOption = canRunInBackground ? (
    <Option
      icon={background ? <Moon size={13} /> : <SquareTerminal size={13} />}
      title={background ? S.runInBackground : S.runInTerminal}
      on
      control={
        <span className="s-run-modes" role="radiogroup">
          <button
            type="button"
            role="radio"
            aria-checked={inBackground}
            aria-label={S.runInBackground}
            title={S.runInBackground}
            className={`s-run-mode${inBackground ? " is-selected" : ""}`}
            onClick={() => setInBackground(true)}
          >
            <Moon size={12} />
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={!inBackground}
            aria-label={S.runInTerminal}
            title={S.runInTerminal}
            className={`s-run-mode${inBackground ? "" : " is-selected"}`}
            onClick={() => setInBackground(false)}
          >
            <SquareTerminal size={12} />
          </button>
        </span>
      }
    />
  ) : (
    <Option icon={<SquareTerminal size={13} />} title={S.runInTerminal} on>
      <span className="s-new-option-note">
        {isHookAgent(agent) ? tf("windows.sessions.opensInTerminal", PROVIDER_NAMES[agent]) : S.backgroundOffHelp}
      </span>
    </Option>
  );

  // Worktree and run mode are one line each, so they share a row and the
  // page fits the island without scrolling.
  const optionsPage = (
    <div className="s-new-options">
      <div className="s-new-option-row">
        {worktreeOption}
        {runOption}
      </div>
      {safeOption}
    </div>
  );

  const typing = prompt.trim() !== "";
  const recipeChip = (target: Step, icon: ReactNode, text: string, label?: string) => (
    <button
      type="button"
      className="s-recipe-chip"
      title={label ?? text}
      aria-label={label ?? text}
      onClick={() => go(target)}
    >
      {icon}
      <span>{text}</span>
    </button>
  );
  const promptPage = (
    <div className="s-prompt-step">
      <span className={`s-prompt-mark${typing ? " is-typing" : ""}`} aria-hidden>
        <ProviderMark provider={agent} size={agent === "claude" ? 24 : 21} working={typing} />
      </span>
      <textarea
        ref={promptField}
        className="s-step-prompt"
        value={prompt}
        rows={3}
        placeholder={background ? S.firstPromptRequired : S.firstPrompt}
        aria-label={background ? S.firstPromptRequired : S.firstPrompt}
        onChange={(event) => setPrompt(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            startOrAsk();
          }
        }}
      />
      <div className="s-recipe">
        {recipeChip("project", <Folder size={11} />, chosen?.name ?? S.stepProject)}
        {recipeChip(
          "agent",
          <ProviderMark provider={agent} size={11} />,
          list.length > 0 ? `${SHORT_NAMES[agent]} · ${modelLabel(model)}` : SHORT_NAMES[agent],
        )}
        {usesWorktree ? recipeChip("options", <GitBranch size={11} />, S.worktreeMark) : null}
        {safe.length > 0
          ? recipeChip("options", <ShieldCheck size={11} />, String(safe.length), S.safeCount(safe.length))
          : null}
        {recipeChip(
          "options",
          background ? <Moon size={11} /> : <SquareTerminal size={11} />,
          background ? S.runInBackground : S.runInTerminal,
        )}
      </div>
    </div>
  );

  const pages: Record<Step, ReactNode> = {
    project: projectPage,
    agent: agentPage,
    options: optionsPage,
    prompt: promptPage,
  };

  return (
    <div className="s-new-session s-steps" data-agent={agent}>
      {stepBar}
      <div className="s-step-stage">
        <div key={step} className={`s-step-page ${forward ? "from-right" : "from-left"}`}>
          <div className="s-step-content">{pages[step]}</div>
        </div>
      </div>
      <div className="s-step-footer">
        {index > 0 ? (
          <button
            type="button"
            className="s-step-arrow"
            aria-label={S.previousStep}
            title={S.previousStep}
            onClick={() => go(STEPS[index - 1])}
          >
            <ChevronLeft size={14} strokeWidth={2.5} />
          </button>
        ) : null}
        {problem ? (
          <span key={problem} className="s-new-message s-background-problem" role="alert">
            {problem}
          </span>
        ) : null}
        <span className="s-background-gap" />
        {index < STEPS.length - 1 ? (
          <button
            type="button"
            className="s-step-arrow"
            aria-label={S.nextStep}
            title={S.nextStep}
            disabled={!chosen}
            onClick={() => go(STEPS[index + 1])}
          >
            <ChevronRight size={14} strokeWidth={2.5} />
          </button>
        ) : null}
        <button
          type="button"
          className={`s-new-start${starting ? " is-starting" : ""}${needsPrompt ? " is-waiting" : ""}`}
          disabled={!canStart}
          aria-busy={starting}
          title={needsPrompt ? S.backgroundNeedsPrompt : S.startSession}
          onClick={startOrAsk}
        >
          <span className="s-new-start-text">
            {needsPrompt ? <MessageSquareText size={11} /> : <Play size={10} fill="currentColor" />}
            {S.startSession}
          </span>
          <span className="s-new-spinner" aria-hidden />
        </button>
      </div>
    </div>
  );
}
