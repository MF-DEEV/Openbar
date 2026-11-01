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

export type HostKind = "windowsTerminal" | "vscode" | "conhost" | "app" | "background" | "unknown";

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

export interface Permission {
  requestId: string;
  tool: string;
  summary?: string;
  detail: string;
  offersAlways: boolean;
  since: number;
  questions: Question[];
  fitsOnALine: boolean;
}

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

export type Provider = "claude" | "codex";

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
  phase: Phase;
  phaseSince: number;
  turnStarted?: number;
  lastActive?: number;
  /** When its page was last opened (or a reply or decision sent), by the service. */
  lookedAt?: number;
  waitingFor?: string;
  currentTool?: ToolUse;
  permission?: Permission;
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
