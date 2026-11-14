// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust sessions service sends the island (event "sessions://snapshot").
// Times are epoch seconds. Text a session wrote is absent unless previews
// are on; a permission's detail and questions are always present.

export type Phase =
  "generating" | "runningTool" | "waitingPermission" | "waitingInput" | "background" | "limited" | "finished" | "idle";

/** A turn the plan's usage limit stopped, as the agent recorded it (seconds). */
export interface LimitStop {
  at: number;
  /** When the limit lifts, when the agent said; otherwise the limits reading tells. */
  resetsAt?: number;
  kind?: "session" | "weekly" | "other";
}

export type HostKind =
  "windowsTerminal" | "vscode" | "conhost" | "wezterm" | "tmux" | "kitty" | "app" | "background" | "remote" | "unknown";

export interface ToolUse {
  id: string;
  name: string;
  summary?: string;
  started: number;
  finished?: number;
  failed: boolean;
}

export interface QuestionOption {
  label: string;
  description?: string;
  preview?: string;
}

export interface Question {
  question: string;
  header?: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

/** One line of a file change's diff (ClaudeFileDiff.Line). */
export interface DiffLine {
  kind: "context" | "added" | "removed" | "gap";
  text: string;
  /** In the file: the new number, or the old one for a removed line. */
  number?: number;
  /** What a gap leaves out of the file as read; absent with no file behind it. */
  gap?: DiffGap;
}

/** The unchanged lines a gap leaves out (file_diff.rs Gap). */
export interface DiffGap {
  /** top opens upward from the first change, bottom downward from the last. */
  edge: "top" | "middle" | "bottom";
  /** How many lines it leaves out. */
  count: number;
  /** Kept from its start, right below the lines above it. */
  head: DiffLine[];
  /** Kept from its end, right above the lines below it. */
  tail: DiffLine[];
}

/** What a file change would do to one file (file_diff.rs). */
export interface FileDiff {
  path: string;
  change: "update" | "add" | "delete" | "replace";
  places?: number;
  everyMatch: boolean;
  /** Set for a notebook cell, empty when the edit names none. */
  cell?: string;
  movedTo?: string;
  added: number;
  removed: number;
  lines: DiffLine[];
  /** Lines past the limit, left out. */
  hidden: number;
}

export interface CommandMetadata {
  revision: string;
  complete: boolean;
  truncated: boolean;
  remaining: number | null;
  requiresInspection: boolean;
  available: boolean;
}
export interface CommandInspection {
  revision: string;
  token: string;
  text: string;
}

export interface Permission {
  requestId: string;
  tool: string;
  summary?: string;
  detail: string;
  command?: CommandMetadata | null;
  offersAlways: boolean;
  since: number;
  questions: Question[];
  /** A file change's diff, per file; empty for anything else. */
  diffs?: FileDiff[];
  /** For a plan, the modes it can be approved into, the one a plain approval picks first. */
  planModes?: PlanMode[];
  fitsOnALine: boolean;
}

/** A mode a plan is approved into, by the name Claude Code takes (ClaudePlanMode). */
export type PlanMode = "bypassPermissions" | "auto" | "acceptEdits" | "default";

export interface Subagent {
  id: string;
  type: string;
  description?: string;
  started: number;
  finished?: number;
  /** The agent definition's `color:`, for its Clawd's shell. */
  color?: string;
}

export interface Tasks {
  done: number;
  total: number;
  current?: string;
}

export interface Tokens {
  input: number;
  cacheWrite: number;
  cacheRead: number;
  output: number;
}

export interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  text: string;
  images?: string[];
  at: number;
  endsTurn: boolean;
}

export type Provider = "claude" | "codex" | "gemini" | "copilot" | "opencode" | "cursor" | "pi" | "vibe" | "devin";

/** Every agent, in the order the strip and lists take them. */
export const PROVIDERS: readonly Provider[] = [
  "claude",
  "codex",
  "gemini",
  "copilot",
  "opencode",
  "cursor",
  "pi",
  "vibe",
  "devin",
];

/** The agents that report through the island's hooks or plugin
 * (sessions/hook_agents.rs, docs/more-session-agents.md). */
export const HOOK_AGENTS = ["gemini", "copilot", "opencode", "cursor", "pi", "vibe", "devin"] as const;
export type HookAgent = (typeof HOOK_AGENTS)[number];

/** The agent's name where room is short, as New session's picker. */
export const SHORT_NAMES: Readonly<Record<Provider, string>> = {
  claude: "Claude",
  codex: "Codex",
  gemini: "Gemini",
  copilot: "Copilot",
  opencode: "OpenCode",
  cursor: "Cursor",
  pi: "Pi",
  vibe: "Vibe",
  devin: "Devin",
};

export const isHookAgent = (provider: Provider): provider is HookAgent =>
  (HOOK_AGENTS as readonly string[]).includes(provider);

/** What the island calls each agent; product names, the same in every language. */
export const PROVIDER_NAMES: Readonly<Record<Provider, string>> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  copilot: "Copilot CLI",
  opencode: "OpenCode",
  cursor: "Cursor",
  pi: "Pi",
  vibe: "Mistral Vibe",
  devin: "Devin CLI",
};

export interface Session {
  id: string;
  /** Claude Code or Codex: each has its own figure and channel. */
  provider: Provider;
  pid: number;
  cwd: string;
  project: string;
  name: string;
  nameSource?: string;
  branch?: string;
  model?: string;
  modelName?: string;
  host: HostKind;
  acceptsReplies: boolean;
  /** The app holding the session as the person knows it, like "WezTerm" or "Git Bash". */
  hostName?: string;
  /** The SSH alias of the host it runs on, for a session on another machine. */
  remote?: string;
  phase: Phase;
  phaseSince: number;
  turnStarted?: number;
  lastActive?: number;
  /** When its page was last opened (or a reply or decision sent), by the service. */
  lookedAt?: number;
  waitingFor?: string;
  currentTool?: ToolUse;
  permission?: Permission;
  /** A file change only the agent can answer, shown above "answer it there". */
  waitingDiffs?: FileDiff[];
  timeline: ToolUse[];
  subagents: Subagent[];
  tasks?: Tasks;
  tokens: Tokens;
  cost: number;
  title?: string;
  lastText?: string;
  hooked: boolean;
  chat: ChatMessage[];
  backgroundLabel?: string;
  services: number;
  /** Set for a whole session running in the background, with no terminal. */
  background?: BackgroundDetails;
  /** Why the session stopped, while `phase` is "limited". */
  limitStop?: LimitStop;
  /** The config folder of the Claude Code account it runs under, on this PC (#216). */
  account?: string;
  /** That account's identity, as the limits name it. */
  accountId?: string;
  /** That account's name, only while sessions of more than one account are listed. */
  accountLabel?: string;
}

/** A Claude Code account: a config folder with its own sign-in (#216; hooks/mod.rs AccountStatus). */
export interface ClaudeAccount {
  id: string;
  path: string;
  name: string;
  named: boolean;
  manual: boolean;
  hidden: boolean;
  hooks: "installed" | "outdated" | "missing" | "unreadable";
}

export type BackgroundState = "working" | "blocked" | "done" | "failed" | "stopped";

export interface BackgroundDetails {
  job: string;
  state?: BackgroundState;
  /** Its process is gone; Open brings it back. */
  ended: boolean;
  configHome?: string;
}

/** A folder Claude Code has worked in, to start a new session there. */
export interface RecentFolder {
  path: string;
  name: string;
  configHome?: string;
  used?: number;
}

export interface Counts {
  working: number;
  /** Of `working`, the Codex ones. */
  workingCodex: number;
  /** Of `working`, each hook agent's; absent from an older service. */
  workingGemini?: number;
  workingCopilot?: number;
  workingOpencode?: number;
  workingCursor?: number;
  workingPi?: number;
  workingVibe?: number;
  workingDevin?: number;
  waiting: number;
  /** Stopped by the plan's usage limit; absent from an older service. */
  limited?: number;
  finished: number;
}

export interface Snapshot {
  sessions: Session[];
  counts: Counts;
  previews: boolean;
  /** Claude Code is installed, so the island can start sessions in a terminal. */
  canStart?: boolean;
  /** Its Agent View is on, so it can run them in the background too. */
  backgroundAvailable?: boolean;
  /** Codex is installed, so New session can start it too. */
  canStartCodex?: boolean;
  /** Gemini CLI, Copilot CLI, OpenCode and cursor-agent found on this PC, which New session starts in a console. */
  startableAgents?: HookAgent[];
  hooksInstalled: boolean;
}

export type SessionEvent =
  | { kind: "needsPermission"; id: string; tool: string }
  | { kind: "needsInput"; id: string }
  | { kind: "finished"; id: string; duration?: number };

export type Decision = "allow" | "always" | "deny";

export type ReplyFailure = "waiting" | "unreachable";

export interface ReplyAttachment {
  path: string;
  preview: string;
}

export const isWorking = (p: Phase) => p === "generating" || p === "runningTool";
export const isActive = (p: Phase) => isWorking(p) || p === "background";
export const isWaiting = (p: Phase) => p === "waitingPermission" || p === "waitingInput";

/** A model a new session can start with; `id` null keeps the agent's own
 * default, which `detail` names when it can be told. */
export interface NewSessionModel {
  id: string | null;
  name: string;
  detail: string | null;
}

export interface NewSessionModels {
  claude: NewSessionModel[];
  codex: NewSessionModel[];
}

/** How many of one agent's sessions work: Claude's are the ones no other agent claims. */
export function workingOf(counts: Counts, provider: Provider): number {
  const total = Math.max(0, counts.working);
  const others: Record<Exclude<Provider, "claude">, number> = {
    codex: counts.workingCodex,
    gemini: counts.workingGemini ?? 0,
    copilot: counts.workingCopilot ?? 0,
    opencode: counts.workingOpencode ?? 0,
    cursor: counts.workingCursor ?? 0,
    pi: counts.workingPi ?? 0,
    vibe: counts.workingVibe ?? 0,
    devin: counts.workingDevin ?? 0,
  };
  if (provider === "claude") {
    return Math.max(0, total - Object.values(others).reduce((sum, n) => sum + Math.max(0, n), 0));
  }
  return Math.min(Math.max(0, others[provider]), total);
}

/** What a folder is to git, for the New session page (worktree.rs). */
export type RepoState = { kind: "notRepo" } | { kind: "clean"; root: string } | { kind: "dirty"; root: string };

/** How removing a session or its worktree went. */
export type WorktreeRemoval = { kind: "done" } | { kind: "unsaved" } | { kind: "failed"; message: string };
