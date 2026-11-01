// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the AI page draws: the latest snapshot from agent_usage (spec 03 §8).

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import type { AgentEvent, AgentSnapshot, PeriodUsage, Totals } from "./types";

export const emptyTotals = (): Totals => ({
  tokens: { input: 0, cacheWrite: 0, cacheRead: 0, output: 0, reasoning: 0 },
  cost: 0,
  savings: 0,
  requests: 0,
  unpriced: 0,
});

const emptyPeriod = (): PeriodUsage => ({ total: emptyTotals(), byProvider: {}, models: [], projects: [] });

export function emptySnapshot(): AgentSnapshot {
  return {
    loaded: false,
    now: 0,
    periods: { today: emptyPeriod(), week: emptyPeriod(), month: emptyPeriod() },
    days: [],
    hours: [],
    limits: {},
    plans: {},
    live: [],
    claudeBlock: null,
    burnRate: {},
    lastActivity: {},
    seen: [],
    found: { claude: false, codex: false, opencode: false, copilot: false },
    claude: { accountAt: null, appAt: null, appInstalled: false, failure: null },
    resets: { checking: false, count: null, credits: [], failure: null, checkedAt: null },
    codexAccounts: [],
    priceDay: "",
    accounts: {},
  };
}

interface AgentsState {
  snapshot: AgentSnapshot;
  receive(snapshot: AgentSnapshot): void;
}

export const useAgents = create<AgentsState>((set) => ({
  snapshot: emptySnapshot(),
  receive: (snapshot) => set({ snapshot }),
}));

/** Listens first, so the first fetch never overwrites a newer event. */
export async function subscribeAgents(onEvent: (event: AgentEvent) => void) {
  let version = 0;
  await bridge.agents.onSnapshot((snapshot) => {
    version++;
    useAgents.getState().receive(snapshot);
  });
  await bridge.agents.onEvent(onEvent);
  const at = version;
  const first = await bridge.agents.snapshot();
  if (first && version === at) useAgents.getState().receive(first);
}
