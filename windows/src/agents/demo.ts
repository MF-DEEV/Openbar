// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample usage for designing the AI page in a plain browser (?agents).

import { emptySnapshot, emptyTotals } from "./store";
import type { AgentSnapshot, Bucket, PeriodUsage, Provider, Share, Totals } from "./types";

/** A day's or hour's totals from a token count and a price per million. */
function totals(tokensCount: number, perMillion: number, requests: number): Totals {
  const t = emptyTotals();
  t.tokens = {
    input: Math.round(tokensCount * 0.04),
    cacheWrite: Math.round(tokensCount * 0.06),
    cacheRead: Math.round(tokensCount * 0.86),
    output: Math.round(tokensCount * 0.04),
    reasoning: Math.round(tokensCount * 0.01),
  };
  t.cost = (tokensCount / 1_000_000) * perMillion;
  t.savings = t.cost * 0.6;
  t.requests = requests;
  return t;
}

function add(into: Totals, t: Totals) {
  into.tokens.input += t.tokens.input;
  into.tokens.cacheWrite += t.tokens.cacheWrite;
  into.tokens.cacheRead += t.tokens.cacheRead;
  into.tokens.output += t.tokens.output;
  into.tokens.reasoning += t.tokens.reasoning;
  into.cost += t.cost;
  into.savings += t.savings;
  into.requests += t.requests;
  into.unpriced += t.unpriced;
}

/** A steady pseudo-random series, so screenshots stay the same. */
function noise(seed: number): number {
  const x = Math.sin(seed * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

function period(days: Bucket[], count: number): PeriodUsage {
  const usage: PeriodUsage = { total: emptyTotals(), byProvider: {}, models: [], projects: [] };
  for (const day of days.slice(-count)) {
    for (const [provider, t] of Object.entries(day.byProvider) as [Provider, Totals][]) {
      add(usage.total, t);
      add((usage.byProvider[provider] ??= emptyTotals()), t);
    }
  }
  const share = (id: string, name: string, provider: Provider | null, part: number): Share => {
    const t = emptyTotals();
    const source = provider ? usage.byProvider[provider] : usage.total;
    if (source) {
      t.tokens = {
        input: source.tokens.input * part,
        cacheWrite: source.tokens.cacheWrite * part,
        cacheRead: source.tokens.cacheRead * part,
        output: source.tokens.output * part,
        reasoning: source.tokens.reasoning * part,
      };
      t.cost = source.cost * part;
      t.requests = Math.round(source.requests * part);
    }
    return { id, name, provider, totals: t };
  };
  usage.models = [
    share("claude:Opus 5.5", "Opus 5.5", "claude", 0.78),
    share("codex:GPT-5.6", "GPT-5.6", "codex", 1),
    share("claude:Sonnet 5.5", "Sonnet 5.5", "claude", 0.22),
  ].sort((a, b) => b.totals.cost - a.totals.cost);
  usage.projects = [
    share("vorssaint", "vorssaint", null, 0.62),
    share("island-windows", "island-windows", null, 0.27),
    share("dotfiles", "dotfiles", null, 0.11),
  ];
  return usage;
}

export function demoSnapshot(now = Date.now(), accounts = false): AgentSnapshot {
  const snapshot = emptySnapshot();
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const days: Bucket[] = [];
  for (let i = 90; i >= 0; i--) {
    const start = new Date(midnight);
    start.setDate(start.getDate() - i);
    const weekend = start.getDay() === 0 || start.getDay() === 6;
    const quiet = noise(i) < (weekend ? 0.55 : 0.12);
    const byProvider: Partial<Record<Provider, Totals>> = {};
    if (!quiet) {
      byProvider.claude = totals(8_000_000 + noise(i + 1) * 30_000_000, 0.9, 120 + Math.round(noise(i + 2) * 300));
      if (noise(i + 3) > 0.4) byProvider.codex = totals(2_000_000 + noise(i + 4) * 9_000_000, 0.7, 40);
    }
    days.push({ start: start.getTime(), byProvider });
  }
  const hours: Bucket[] = [];
  const hour = new Date(now).getHours();
  for (let h = 0; h < 24; h++) {
    const byProvider: Partial<Record<Provider, Totals>> = {};
    if (h >= 8 && h <= hour && noise(h + 40) > 0.2) {
      byProvider.claude = totals(400_000 + noise(h) * 2_400_000, 0.9, 12);
      if (noise(h + 7) > 0.5) byProvider.codex = totals(200_000 + noise(h + 9) * 900_000, 0.7, 5);
    }
    hours.push({ start: midnight.getTime() + h * 3_600_000, byProvider });
  }
  // Today is the sum of its hours.
  days[days.length - 1] = { start: midnight.getTime(), byProvider: {} };
  for (const h of hours)
    for (const [provider, t] of Object.entries(h.byProvider) as [Provider, Totals][])
      add((days[days.length - 1]!.byProvider[provider] ??= emptyTotals()), t);

  snapshot.loaded = true;
  snapshot.now = now;
  snapshot.days = days;
  snapshot.hours = hours;
  snapshot.periods = { today: period(days, 1), week: period(days, 7), month: period(days, 30) };
  snapshot.seen = ["claude", "codex"];
  snapshot.found = { claude: true, codex: true, opencode: false, copilot: false };
  snapshot.accounts = { claude: "ada@example.com", codex: "ada@studio.example" };
  snapshot.plans = {
    claude: { name: "Max 20×", monthlyPrice: 200 },
    codex: { name: "Plus", monthlyPrice: 20 },
  };
  snapshot.limits = {
    claude: {
      provider: "claude",
      observedAt: now - 40_000,
      source: "account",
      windows: [
        {
          id: "claude.fh",
          kind: "session",
          minutes: 300,
          scope: null,
          usedPercent: 42,
          resetsAt: now + 2 * 3_600_000 + 13 * 60_000,
          estimatedResetsAt: null,
        },
        {
          id: "claude.sd",
          kind: "weekly",
          minutes: 10_080,
          scope: null,
          usedPercent: 83,
          resetsAt: now + 2 * 86_400_000 + 5 * 3_600_000,
          estimatedResetsAt: null,
        },
      ],
    },
    codex: {
      provider: "codex",
      observedAt: now - 120_000,
      source: "sessionLog",
      windows: [
        {
          id: "codex.300",
          kind: "session",
          minutes: 300,
          scope: null,
          usedPercent: 9,
          resetsAt: now + 4 * 3_600_000,
          estimatedResetsAt: null,
        },
        {
          id: "codex.10080",
          kind: "weekly",
          minutes: 10_080,
          scope: null,
          usedPercent: 96,
          resetsAt: now + 5 * 86_400_000,
          estimatedResetsAt: null,
        },
      ],
    },
  };
  snapshot.live = [
    {
      id: "C:\\Users\\me\\.claude\\projects\\vorssaint\\session.jsonl",
      provider: "claude",
      started: now - 754_000,
      lastActivity: now - 2_000,
      model: "claude-opus-5-5",
      project: "vorssaint",
      tokens: { input: 18_000, cacheWrite: 120_000, cacheRead: 1_040_000, output: 20_400, reasoning: 3_000 },
      cost: 4.56,
    },
  ];
  snapshot.lastActivity = { claude: now - 2_000, codex: now - 50 * 60_000 };
  snapshot.claude = { accountAt: now - 40_000, appAt: null, appInstalled: false, failure: null };
  snapshot.resets = {
    checking: false,
    count: 2,
    credits: [
      { id: "credit-1", expiresAt: now + 20 * 3_600_000 },
      { id: "credit-2", expiresAt: now + 9 * 86_400_000 },
    ],
    failure: null,
    checkedAt: now - 60_000,
  };
  snapshot.priceDay = "2026-10-09";
  if (accounts) snapshot.codexAccounts = demoAccounts(now);
  return snapshot;
}

/** Added Codex homes: one read, one being checked, one signed out and one whose last check failed. */
function demoAccounts(now: number): AgentSnapshot["codexAccounts"] {
  const window = (id: string, kind: "session" | "weekly", minutes: number, used: number, resetsAt: number) => ({
    id,
    kind,
    minutes,
    scope: null,
    usedPercent: used,
    resetsAt,
    estimatedResetsAt: null,
  });
  return [
    {
      id: "work",
      name: "Work",
      plan: "Pro",
      checking: false,
      failure: null,
      limits: {
        provider: "codex",
        observedAt: now - 60_000,
        source: "account",
        windows: [
          window("codex.300", "session", 300, 34, now + 2 * 3_600_000),
          window("codex.10080", "weekly", 10080, 82, now + 3 * 86_400_000),
        ],
      },
    },
    { id: "personal", name: "Personal", plan: null, checking: true, failure: null, limits: null },
    { id: "client", name: "Client", plan: null, checking: false, failure: "needsSignIn", limits: null },
    {
      id: "old",
      name: "Old",
      plan: "Plus",
      checking: false,
      failure: "unavailable",
      limits: {
        provider: "codex",
        observedAt: now - 25 * 60_000,
        source: "account",
        windows: [window("codex.300", "session", 300, 12, now + 4 * 3_600_000)],
      },
    },
  ];
}
