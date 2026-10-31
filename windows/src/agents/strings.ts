// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI Agents section's words: the Mac's English (NotchAgentStrings.swift
// :191-288), with "this Mac" said as "this PC" and the menu bar as the
// notification area.

import { t, tf, getLanguage, pluralForm } from "../i18n";

import type { AgentCard, AgentPeriod, AgentReadout, LimitFocus } from "./types";

export const A = {
  get title() {
    return t("notchAgents.title");
  },
  get limitsCard() {
    return t("notchAgents.limitsCard");
  },
  get spendCard() {
    return t("notchAgents.spendCard");
  },
  get liveCard() {
    return t("notchAgents.liveCard");
  },
  get trendCard() {
    return t("notchAgents.trendCard");
  },
  get modelsCard() {
    return t("notchAgents.modelsCard");
  },
  get projectsCard() {
    return t("notchAgents.projectsCard");
  },
  get activityCard() {
    return t("notchAgents.activityCard");
  },
  get resetsCard() {
    return t("notchAgents.resetsCard");
  },
  get today() {
    return t("notchAgents.today");
  },
  get week() {
    return t("notchAgents.week");
  },
  get month() {
    return t("notchAgents.month");
  },
  get period() {
    return t("windows.agents.period");
  },
  get session() {
    return t("notchAgents.session");
  },
  get weekly() {
    return t("notchAgents.weekly");
  },
  get estimated() {
    return t("windows.agents.estimated");
  },
  get claudeLimitsHint() {
    return t("notchAgents.claudeLimitsHint");
  },
  get waitingForLimits() {
    return t("notchAgents.waitingForLimits");
  },
  get noSession() {
    return t("notchAgents.noSession");
  },
  get apiValue() {
    return t("notchAgents.apiValue");
  },
  tokens: (count: string) => tf("notchAgents.tokensFormat", count),
  cached: (share: string) => tf("notchAgents.cachedFormat", share),
  written: (count: string) => tf("notchAgents.writtenFormat", count),
  saved: (amount: string) => tf("notchAgents.savedFormat", amount),
  planMultiple: (multiple: string, plan: string) => tf("notchAgents.planMultipleFormat", multiple, plan),
  get idle() {
    return t("notchAgents.idle");
  },
  get noActivity() {
    return t("notchAgents.noActivity");
  },
  get empty() {
    return t("windows.agents.empty");
  },
  get loading() {
    return t("notchAgents.loading");
  },
  get noCards() {
    return t("notchAgents.noCards");
  },
  get unpriced() {
    return t("notchAgents.unpriced");
  },
  get streak() {
    return t("notchAgents.streak");
  },
  get activeDays() {
    return t("notchAgents.activeDays");
  },
  get busiestDay() {
    return t("notchAgents.busiestDay");
  },
  weeks: (count: number) => tf(`windows.units.week.${pluralForm(getLanguage(), count)}`, count),
  finished: (agent: string) => tf("notchAgents.finishedFormat", agent),
  get limitRenewed() {
    return t("notchAgents.limitRenewed");
  },
  get budgetTitle() {
    return t("notchAgents.budgetTitle");
  },
  left: (share: string) => tf("notchAgents.leftFormat", share),
  used: (share: string) => tf("notchAgents.usedFormat", share),
  updated: (when: string) => tf("notchAgents.updatedFormat", when),
  claudeCodeUpdated: (when: string) => tf("windows.agents.claudeCodeUpdated", when),
  seconds: (n: number) => tf("windows.agents.seconds", n),
  minutes: (n: number) => tf("windows.agents.minutes", n),
  get readoutLimit() {
    return t("notchAgents.readoutLimit");
  },
  get valueNote() {
    return t("notchAgents.valueNote");
  },
  // Resets (Codex's banked resets)
  get useReset() {
    return t("notchAgents.useReset");
  },
  get resetConfirm() {
    return t("notchAgents.resetConfirm");
  },
  get confirmReset() {
    return t("notchAgents.confirmReset");
  },
  get cancel() {
    return t("clipboard.cancel");
  },
  get retry() {
    return t("windows.agents.retry");
  },
  get resetting() {
    return t("notchAgents.resetting");
  },
  get resetDone() {
    return t("notchAgents.resetDone");
  },
  get resetNotNeeded() {
    return t("notchAgents.resetNotNeeded");
  },
  get resetTaken() {
    return t("notchAgents.resetTaken");
  },
  get resetFailed() {
    return t("notchAgents.resetFailed");
  },
  get resetsNone() {
    return t("notchAgents.resetsNone");
  },
  resetsExpiry: (when: string) => tf("notchAgents.resetsExpiryFormat", when),
  get resetsNeedCodex() {
    return t("notchAgents.resetsNeedCodex");
  },
  get resetsSignIn() {
    return t("notchAgents.resetsSignIn");
  },
  get resetsUpdate() {
    return t("notchAgents.resetsUpdate");
  },
  get resetsCheckFailed() {
    return t("notchAgents.resetsCheckFailed");
  },
  get resetsHelp() {
    return t("notchAgents.resetsHelp");
  },
  // Codex accounts (CodexAccountStrings.swift:33-51)
  get defaultAccount() {
    return t("codexAccounts.defaultAccount");
  },
  get accountRefreshHint() {
    return t("codexAccounts.refreshHint");
  },
  get accountCheck() {
    return t("codexAccounts.check");
  },
  get accountFolderProblem() {
    return t("codexAccounts.folderProblem");
  },
  get accountSignIn() {
    return t("codexAccounts.signIn");
  },
  get accountUpdate() {
    return t("codexAccounts.update");
  },
  get accountFailed() {
    return t("codexAccounts.failed");
  },
  get codexAccountsTitle() {
    return t("codexAccounts.title");
  },
  get codexAccountsExplanation() {
    return t("codexAccounts.explanation");
  },
  get accountName() {
    return t("codexAccounts.accountName");
  },
  get removeAccount() {
    return t("codexAccounts.remove");
  },
  get findAccounts() {
    return t("codexAccounts.find");
  },
  get addFolder() {
    return t("codexAccounts.add");
  },
  get accountsFallback() {
    return t("codexAccounts.fallback");
  },
  get accountsResetsNote() {
    return t("codexAccounts.resetsNote");
  },
  get chooseCodexFolder() {
    return t("codexAccounts.chooseFolder");
  },
  get invalidCodexFolder() {
    return t("codexAccounts.invalidFolder");
  },
  /** The name the usual `.codex` home gets. */
  get defaultName() {
    return t("codexAccounts.defaultName");
  },
  // Settings
  get settingsDescription() {
    return t("windows.agents.settingsDescription");
  },
  get found() {
    return t("windows.agents.found");
  },
  get notFound() {
    return t("windows.agents.notFound");
  },
  get cardsTitle() {
    return t("notchAgents.cardsTitle");
  },
  get cardsHint() {
    return t("notchAgents.cardsHint");
  },
  get limitsAs() {
    return t("notchAgents.limitsAs");
  },
  get remaining() {
    return t("notchAgents.remaining");
  },
  get usedChoice() {
    return t("notchAgents.used");
  },
  get limitFocus() {
    return t("notchAgents.limitFocus");
  },
  get mostUsed() {
    return t("notchAgents.mostUsed");
  },
  get liveTitle() {
    return t("notchAgents.liveTitle");
  },
  get liveActivity() {
    return t("notchAgents.liveActivity");
  },
  get readout() {
    return t("notchAgents.readout");
  },
  get readoutElapsed() {
    return t("notchAgents.readoutElapsed");
  },
  get readoutTokens() {
    return t("notchAgents.readoutTokens");
  },
  get alerts() {
    return t("notchAgents.alerts");
  },
  get finishAlert() {
    return t("notchAgents.finishAlert");
  },
  get finishAfter() {
    return t("notchAgents.finishAfter");
  },
  get anyLength() {
    return t("notchAgents.anyLength");
  },
  get limitAlert() {
    return t("notchAgents.limitAlert");
  },
  get limitAt() {
    return t("notchAgents.limitAt");
  },
  get budget() {
    return t("notchAgents.budget");
  },
  get off() {
    return t("notchAgents.off");
  },
  get claudeLimitsTitle() {
    return t("notchAgents.claudeLimitsTitle");
  },
  claudeLimitsCurrent: (when: string) => tf("notchAgents.claudeLimitsCurrentFormat", when),
  claudeLimitsStale: (when: string) => tf("notchAgents.claudeLimitsStaleFormat", when),
  get claudeLimitsTray() {
    return t("windows.agents.claudeLimitsTray");
  },
  get claudeLimitsNoApp() {
    return t("windows.agents.claudeLimitsNoApp");
  },
  get claudeLimitsPrivacy() {
    return t("windows.agents.claudeLimitsPrivacy");
  },
  get claudeCodeMissing() {
    return t("windows.agents.claudeCodeMissing");
  },
  get claudeCodeSignedOut() {
    return t("notchAgents.claudeCodeSignedOut");
  },
  get claudeCodeOutdated() {
    return t("notchAgents.claudeCodeOutdated");
  },
  get claudeCodeUnreachable() {
    return t("notchAgents.claudeCodeUnreachable");
  },
  get getClaude() {
    return t("notchAgents.getClaude");
  },
  get priceUpdates() {
    return t("notchAgents.priceUpdates");
  },
  get priceUpdatesHint() {
    return t("notchAgents.priceUpdatesHint");
  },
  pricesFrom: (day: string) => tf("notchAgents.pricesFromFormat", day),
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
  ({ elapsed: A.readoutElapsed, tokens: A.readoutTokens, cost: t("notchAgents.apiValue"), limit: A.readoutLimit })[
    readout
  ];

export const focusName = (focus: LimitFocus): string =>
  ({ mostUsed: A.mostUsed, session: A.session, weekly: A.weekly })[focus];
