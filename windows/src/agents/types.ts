// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the Rust side (src-tauri/src/agent_usage) tells the AI page: the
// snapshot AgentUsageSummary.swift builds (spec 03 §8) and the events it
// raises (§9). Dates are milliseconds since 1970.

export type Provider = "claude" | "codex" | "opencode" | "copilot";
export const PROVIDERS: readonly Provider[] = ["claude", "codex", "opencode", "copilot"];

export type AgentPeriod = "today" | "week" | "month";
export type AgentCard = "limits" | "spend" | "live" | "trend" | "models" | "projects" | "activity" | "resets";
export const AGENT_CARDS: readonly AgentCard[] = [
  "limits",
  "spend",
  "live",
  "trend",
  "models",
  "projects",
  "activity",
  "resets",
];
export type AgentReadout = "elapsed" | "tokens" | "cost" | "limit";
export type LimitDisplay = "remaining" | "used";
export type LimitFocus = "mostUsed" | "session" | "weekly";

/** `input` excludes cache traffic; `output` already holds the reasoning. */
export interface Tokens {
  input: number;
  cacheWrite: number;
  cacheRead: number;
  output: number;
  reasoning: number;
}

export interface Totals {
  tokens: Tokens;
  /** API value in USD of the priced responses. */
  cost: number;
  savings: number;
  requests: number;
  /** Responses without a known price, left out of `cost`. */
  unpriced: number;
}

export interface Share {
  id: string;
  name: string;
  provider: Provider | null;
  totals: Totals;
}

export interface PeriodUsage {
  total: Totals;
  byProvider: Partial<Record<Provider, Totals>>;
  /** Sorted by cost when every response was priced, by tokens otherwise. */
  models: Share[];
  projects: Share[];
}

export interface Bucket {
  start: number;
  byProvider: Partial<Record<Provider, Totals>>;
}

export interface LimitWindow {
  id: string;
  kind: "session" | "weekly" | "other";
  minutes: number | null;
  /** A model the allowance applies to ("Opus"); null for the whole plan. */
  scope: string | null;
  usedPercent: number;
  resetsAt: number | null;
  /** A presentation estimate (≈), never a deadline. */
  estimatedResetsAt: number | null;
}

export interface Limits {
  provider: Provider;
  windows: LimitWindow[];
  observedAt: number;
  source: "claudeApp" | "sessionLog" | "account";
}

export interface Plan {
  name: string;
  monthlyPrice: number | null;
}

export interface LiveSession {
  /** The log file the turn is read from. */
  id: string;
  provider: Provider;
  started: number;
  lastActivity: number;
  model: string;
  project: string;
  tokens: Tokens;
  cost: number;
}

export interface Block {
  start: number;
  end: number;
  totals: Totals;
}

export type ClaudeFailure = "missing" | "needsSignIn" | "outdated" | "unreachable";
export type CodexFailure = "missing" | "needsSignIn" | "outdated" | "refused" | "unreachable";

/** "Claude plan limits" in Settings (NotchAgentsSettings.swift:171-208). */
export interface ClaudeStatus {
  /** When Claude Code last answered the account check. */
  accountAt: number | null;
  /** The newest reading of the Claude app's plan-usage history. */
  appAt: number | null;
  /** The Claude desktop app is installed on this PC. */
  appInstalled: boolean;
  /** The last account check's failure, until one succeeds. */
  failure: ClaudeFailure | null;
}

export interface ResetCredit {
  id: string;
  expiresAt: number | null;
}

/** Codex's banked resets (AgentCodexResetService.swift). */
export interface ResetsState {
  checking: boolean;
  /** null until the first answer. */
  count: number | null;
  credits: ResetCredit[];
  failure: CodexFailure | null;
  checkedAt: number | null;
}

/** A Codex home of its own, a `CODEX_HOME`, whose account gets a limits card (CodexAccountProfile). */
export interface CodexProfile {
  id: string;
  name: string;
  directory: string;
}

export type CodexAccountFailure =
  "invalidProfile" | "missing" | "needsSignIn" | "outdated" | "unavailable" | "cancelled";

/** An added Codex home's checks, kept apart from the reading the logs give (CodexProfileLimitsState). */
export interface CodexAccount {
  id: string;
  name: string;
  /** Only its own checks; windows that renewed since are left out. */
  limits: Limits | null;
  plan: string | null;
  /** Waiting for its turn or being checked. */
  checking: boolean;
  failure: CodexAccountFailure | null;
}

export type RedeemOutcome = "reset" | "nothingToReset" | "alreadyRedeemed" | "noCredit" | "failed";

export interface AgentSnapshot {
  loaded: boolean;
  now: number;
  periods: Record<AgentPeriod, PeriodUsage>;
  /** 91 local days, oldest first, ending with today. */
  days: Bucket[];
  /** Today's 24 local hours from midnight. */
  hours: Bucket[];
  limits: Partial<Record<Provider, Limits>>;
  plans: Partial<Record<Provider, Plan>>;
  /** Open turns, oldest start first. */
  live: LiveSession[];
  claudeBlock: Block | null;
  /** The last half hour, scaled to an hour. */
  burnRate: Partial<Record<Provider, Totals>>;
  lastActivity: Partial<Record<Provider, number>>;
  /** Providers with any record, limit or turn. */
  seen: Provider[];
  /** Whether each provider's logs exist on this PC (Settings: "Found on this PC"). */
  found: Record<Provider, boolean>;
  claude: ClaudeStatus;
  resets: ResetsState;
  /** The Codex homes added in Settings, in their order. */
  codexAccounts: CodexAccount[];
  /** The price list's `updated` day, "YYYY-MM-DD". */
  priceDay: string;
}

export type AgentEvent =
  | { kind: "finished"; provider: Provider; duration: number; cost: number; tokens: number; project: string }
  | { kind: "limitWarning"; provider: Provider; window: LimitWindow }
  | { kind: "limitReset"; provider: Provider; window: LimitWindow }
  | { kind: "budgetReached"; spent: number; budget: number };
