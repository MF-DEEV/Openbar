// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { restingLimit } from "./model";
import { emptySnapshot } from "./store";
import type { LimitWindow, Limits } from "./types";

const window = (kind: LimitWindow["kind"], usedPercent: number, resetsAt: number | null = null): LimitWindow => ({
  id: kind,
  kind,
  usedPercent,
  resetsAt,
  estimatedResetsAt: null,
  scope: null,
  minutes: 300,
});
const limits = (provider: Limits["provider"], windows: LimitWindow[]): Limits => ({
  provider,
  windows,
  observedAt: 0,
  source: "account",
});

it("shows no invented allowance when no data is available", () => {
  expect(restingLimit(emptySnapshot(), "mostUsed", 0)).toBeNull();
});

it("prefers the chosen kind across accounts, then falls back to the most spent window", () => {
  const snapshot = emptySnapshot();
  snapshot.limits = {
    claude: limits("claude", [window("session", 20)]),
    codex: limits("codex", [window("weekly", 90)]),
  };
  expect(restingLimit(snapshot, "session", 0)?.provider).toBe("claude");
  expect(restingLimit(snapshot, "weekly", 0)?.provider).toBe("codex");
  delete snapshot.limits.claude;
  expect(restingLimit(snapshot, "session", 0)?.provider).toBe("codex");
});

it("includes added Codex accounts and renews elapsed windows without fetching", () => {
  const snapshot = emptySnapshot();
  snapshot.limits.claude = limits("claude", [window("session", 35)]);
  snapshot.codexAccounts = [
    {
      id: "second",
      name: "Second",
      limits: limits("codex", [window("session", 80, 1000)]),
      plan: null,
      checking: false,
      failure: null,
    },
  ];
  expect(restingLimit(snapshot, "mostUsed", 0)?.provider).toBe("codex");
  expect(restingLimit(snapshot, "mostUsed", 1001)?.provider).toBe("claude");
});
