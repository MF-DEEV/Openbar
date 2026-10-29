// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI Agents section's words: the Mac's English (NotchAgentStrings.swift
// :191-288), with "this Mac" said as "this PC" and the menu bar as the
// notification area.

import type { AgentCard, AgentPeriod, AgentReadout, LimitFocus } from "./types";

export const A = {
  title: "AI Agents",
  limitsCard: "Limits",
  spendCard: "Spending",
  liveCard: "Now",
  trendCard: "Trend",
  modelsCard: "Models",
  projectsCard: "Projects",
  activityCard: "Activity",
  resetsCard: "Resets",
  today: "Today",
  week: "7 days",
  month: "30 days",
  period: "Period",
  session: "Session",
  weekly: "Week",
  estimated: "Estimated from this PC",
  claudeLimitsHint: "Set up plan limits…",
  waitingForLimits: "Limits appear after the next reply",
  noSession: "No session running",
  apiValue: "API value",
  tokens: (count: string) => `${count} tokens`,
  cached: (share: string) => `${share} from cache`,
  written: (count: string) => `${count} written`,
  saved: (amount: string) => `${amount} saved by the cache`,
  planMultiple: (multiple: string, plan: string) => `${multiple} the price of ${plan}`,
  idle: "Idle",
  noActivity: "Nothing in this period",
  empty:
    "No usage from Claude Code, Codex, OpenCode or GitHub Copilot yet. It appears here as soon as any of them works on this PC.",
  loading: "Reading usage…",
  noCards: "Choose what this page shows in Dynamic Island settings.",
  unpriced: "Some models have no known price, so this is a minimum.",
  streak: "Days in a row",
  activeDays: "Active days",
  busiestDay: "Busiest day",
  weeks: (count: number) => `${count} ${count === 1 ? "week" : "weeks"}`,
  finished: (agent: string) => `${agent} finished`,
  limitRenewed: "Limit renewed",
  budgetTitle: "Daily budget",
  left: (share: string) => `${share} left`,
  used: (share: string) => `${share} used`,
  updated: (when: string) => `Updated ${when}`,
  claudeCodeUpdated: (when: string) => `Claude Code · Updated ${when}`,
  seconds: (n: number) => `${n} sec`,
  minutes: (n: number) => `${n} min`,
  readoutLimit: "Limit",
  valueNote: "API value is what the same work would cost at API list prices. Plans charge a fixed price instead.",
  // Resets (Codex's banked resets)
  useReset: "Use a reset",
  resetConfirm: "Reset the session and weekly limits now?",
  confirmReset: "Reset",
  cancel: "Cancel",
  retry: "Retry",
  resetting: "Resetting…",
  resetDone: "Limits reset",
  resetNotNeeded: "Your usage doesn’t need a reset yet",
  resetTaken: "That reset was already used",
  resetFailed: "Couldn’t use the reset",
  resetsNone: "No resets available",
  resetsExpiry: (when: string) => `Next expires ${when}`,
  resetsNeedCodex: "Needs the Codex app or CLI",
  resetsSignIn: "Sign in to Codex with a plan to see resets",
  resetsUpdate: "Update Codex to use resets here",
  resetsCheckFailed: "Couldn’t check resets",
  resetsHelp:
    "A reset renews the Codex session and weekly limits at once. Codex checks your resets with its own sign-in, which Vorssaint never reads.",
  // Codex accounts (CodexAccountStrings.swift:33-51)
  defaultAccount: "Default account",
  accountRefreshHint: "Refresh to check this account’s limits.",
  accountCheck: "Check limits",
  accountFolderProblem: "Choose this account’s folder again in Settings.",
  accountSignIn: "Sign in with ChatGPT in this Codex folder.",
  accountUpdate: "Update Codex to check limits.",
  accountFailed: "Could not check limits. Try again.",
  codexAccountsTitle: "Codex accounts",
  codexAccountsExplanation:
    "Each CODEX_HOME folder you add gets its own limits card. While the AI Agents page is open, Codex checks each one about every five minutes with that folder’s own sign-in. No prompt is sent.",
  accountName: "Account name",
  removeAccount: "Remove this account’s card",
  findAccounts: "Find local accounts",
  addFolder: "Add folder…",
  accountsFallback: "With no accounts added, Codex limits come from its local sessions.",
  accountsResetsNote: "The Resets card still belongs to the default Codex account.",
  chooseCodexFolder: "Choose a Codex folder",
  invalidCodexFolder:
    "Choose a separate CODEX_HOME folder with config.toml or auth.json in it. The folder must be yours and not writable by others, and auth.json must be private and not a link. Codex may refresh its own sign-in and caches there.",
  /** The name the usual `.codex` home gets. */
  defaultName: "Default",
  // Settings
  settingsDescription:
    "Reads the usage Claude Code, Codex, OpenCode and GitHub Copilot record on this PC and the plan limits the Claude app saves. Prompts, replies and files are never kept, and your usage never leaves your PC.",
  found: "Found on this PC",
  notFound: "Not found on this PC",
  cardsTitle: "Cards",
  cardsHint: "Drag to reorder. Charts take the full width of the island.",
  limitsAs: "Show limits as",
  remaining: "Left",
  usedChoice: "Used",
  limitFocus: "Limit to show",
  mostUsed: "Most used",
  liveTitle: "While an agent works",
  liveActivity: "Show it in the closed Dynamic Island",
  readout: "Beside the camera",
  readoutElapsed: "Time",
  readoutTokens: "Tokens written",
  alerts: "Alerts",
  finishAlert: "When a task finishes",
  finishAfter: "For tasks longer than",
  anyLength: "Any length",
  limitAlert: "Near a plan limit",
  limitAt: "Warn at",
  budget: "Daily API value budget",
  off: "Off",
  claudeLimitsTitle: "Claude plan limits",
  claudeLimitsCurrent: (when: string) => `Read from the Claude app, updated ${when}`,
  claudeLimitsStale: (when: string) => `The Claude app last checked them ${when}.`,
  claudeLimitsTray:
    "Claude checks your limits only while its icon is in the notification area. Turn the icon on in the Claude app settings, and your limits appear here within minutes.",
  claudeLimitsNoApp:
    "Plan limits come from Claude Code or from the Claude app, which is not on this PC. Until then, the 5-hour session is estimated from Claude Code activity.",
  claudeLimitsPrivacy:
    "Claude Code checks your account’s limits, including use on other machines, with its own sign-in, which Vorssaint never reads. No prompt is sent. When it cannot check, the percentages the Claude app saves on this PC are used.",
  claudeCodeMissing: "Claude Code was not found on this PC.",
  claudeCodeSignedOut: "Sign in to Claude Code so it can check your account’s limits.",
  claudeCodeOutdated: "Update Claude Code so it can check your account’s limits.",
  claudeCodeUnreachable: "Claude Code could not check your account’s limits. It tries again later.",
  getClaude: "Get the Claude app",
  priceUpdates: "Keep prices up to date",
  priceUpdatesHint:
    "Downloads the public price list from GitHub once a day, so new models get an API value without an app update. Nothing about your usage is sent.",
  pricesFrom: (day: string) => `Price list of ${day}`,
};

export const periodName = (period: AgentPeriod): string =>
  period === "today" ? A.today : period === "week" ? A.week : A.month;

export const cardName = (card: AgentCard): string =>
  ({
    limits: A.limitsCard,
    spend: A.spendCard,
    live: A.liveCard,
    trend: A.trendCard,
    models: A.modelsCard,
    projects: A.projectsCard,
    activity: A.activityCard,
    resets: A.resetsCard,
  })[card];

/** "API value" capitalised as a choice (NotchAgentStrings.readout). */
export const readoutName = (readout: AgentReadout): string =>
  ({ elapsed: A.readoutElapsed, tokens: A.readoutTokens, cost: "API value", limit: A.readoutLimit })[readout];

export const focusName = (focus: LimitFocus): string =>
  ({ mostUsed: A.mostUsed, session: A.session, weekly: A.weekly })[focus];
