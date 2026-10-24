// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust sessions service sends the island (event "sessions://snapshot").
// Times are epoch seconds. Text a session wrote is absent unless previews
// are on; a permission's detail and questions are always present.

export type Phase =
  "generating" | "runningTool" | "waitingPermission" | "waitingInput" | "background" | "finished" | "idle";

export type HostKind = "windowsTerminal" | "vscode" | "conhost" | "app" | "unknown";

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
  at: number;
  endsTurn: boolean;
}

export interface Session {
  id: string;
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
}

export interface Counts {
  working: number;
  waiting: number;
  finished: number;
}

export interface Snapshot {
  sessions: Session[];
  counts: Counts;
  previews: boolean;
  hooksInstalled: boolean;
}

export type SessionEvent =
  | { kind: "needsPermission"; id: string; tool: string }
  | { kind: "needsInput"; id: string }
  | { kind: "finished"; id: string; duration?: number };

export type Decision = "allow" | "always" | "deny";

export type ReplyFailure = "waiting" | "unreachable";

export const isWorking = (p: Phase) => p === "generating" || p === "runningTool";
export const isActive = (p: Phase) => isWorking(p) || p === "background";
export const isWaiting = (p: Phase) => p === "waitingPermission" || p === "waitingInput";
