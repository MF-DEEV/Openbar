// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// SessionLimitsTests.swift: the plan limits beside the sessions.

import { describe, expect, it } from "vitest";
import { emptySnapshot } from "../agents/store";
import type { AgentSnapshot, LimitWindow } from "../agents/types";
import { demoSnapshot } from "./demo";
import { activityText, LIMITED_TINT, phaseTint } from "./format";
import { GaugeFill } from "./glyphs";
import { limitReset, resetText, runningSessions, stripLimit, stripShare } from "./limits";
import { countItems, readingWidth, stripWing } from "./SessionsStrip";
import type { Session } from "./types";
import { motionForPhase } from "./clawd/sprite";

const T = 1_792_600_000_000; // ms
const window = (id: string, kind: LimitWindow["kind"], used: number, minutes: number | null, resetsAt: number | null) =>
  ({ id, kind, minutes, scope: null, usedPercent: used, resetsAt, estimatedResetsAt: null }) satisfies LimitWindow;

function snapshot(): AgentSnapshot {
  const s = emptySnapshot();
  s.limits.claude = {
    provider: "claude",
    windows: [
      window("claude.weekly", "weekly", 97, 10_080, T + 2 * 86_400_000),
      window("claude.session", "session", 30, 300, T + 3_600_000),
      { ...window("claude.opus", "weekly", 99, 10_080, T + 9_000_000), scope: "Opus" },
    ],
    observedAt: T,
    source: "account",
  };
  s.limits.codex = {
    provider: "codex",
    windows: [window("codex.primary", "session", 100, 300, T + 1_800_000)],
    observedAt: T,
    source: "sessionLog",
  };
  s.limits.opencode = {
    provider: "opencode",
    windows: [window("opencode.x", "session", 99, 300, T + 60_000)],
    observedAt: T,
    source: "sessionLog",
  };
  return s;
}

const base = demoSnapshot().sessions[0]!;
const session = (patch: Partial<Session>): Session => ({ ...base, ...patch });

describe("plan limits beside the sessions", () => {
  it("puts a limit on the strip only past the threshold, a spent weekly one before a quiet chosen one", () => {
    const planWide = snapshot();
    planWide.limits.claude!.windows = planWide.limits.claude!.windows.filter((w) => w.scope === null);
    expect(stripLimit(planWide, new Set(["claude"]), "session", 80, T + 1000)?.window.id).toBe("claude.weekly");
    expect(stripLimit(planWide, new Set(["claude"]), "session", 99, T + 1000)).toBeNull();
    expect(stripLimit(snapshot(), new Set(["claude", "codex"]), "mostUsed", 80, T + 1000)?.provider).toBe("codex");
    expect(stripLimit(snapshot(), new Set(["opencode"]), "mostUsed", 1, T + 1000)).toBeNull();
    expect(stripShare(window("w", "session", 92, 300, null), "remaining")).toBe("8%");
    expect(stripShare(window("w", "session", 92, 300, null), "used")).toBe("92%");
  });

  it("tells when a stopped session goes on: its own reset, else the spent window's", () => {
    const now = T / 1000 + 1;
    const codex = session({ provider: "codex", phase: "limited", limitStop: { at: T / 1000 } });
    expect(limitReset(codex, snapshot(), now)).toBe(T / 1000 + 1800);
    const own = { ...codex, limitStop: { at: T / 1000, resetsAt: T / 1000 + 500 } };
    expect(limitReset(own, snapshot(), now)).toBe(T / 1000 + 500);
    expect(limitReset(own, snapshot(), T / 1000 + 600)).toBeNull();
    expect(limitReset({ ...codex, limitStop: undefined }, snapshot(), now)).toBeNull();
  });

  it("writes the reset as short as it can be told", () => {
    const noon = Date.UTC(2026, 10, 2, 12) / 1000; // a Monday
    expect(resetText(noon + 7200, noon, "en-US", "UTC")).toBe("2:00 PM");
    expect(resetText(noon + 2 * 86_400, noon, "en-US", "UTC")).toBe("Wed 12:00 PM");
    expect(resetText(noon + 9 * 86_400, noon, "en-US", "UTC")).toBe("Nov 11, 12:00 PM");
  });

  it("counts the warned agent's sessions under way, not stopped ones or the other agent's", () => {
    const list = [
      session({ provider: "codex", phase: "limited" }),
      session({ provider: "codex", phase: "generating" }),
      session({ provider: "codex", phase: "waitingInput" }),
      session({ provider: "claude", phase: "runningTool" }),
    ];
    expect(runningSessions(list, "codex")).toBe(2);
    expect(runningSessions(list, "claude")).toBe(1);
  });
});

describe("a session the plan's limit stopped", () => {
  it("reads as stopped, with when it goes on", () => {
    const stopped = session({ phase: "limited", title: undefined });
    expect(activityText(stopped, stopped.name, "2:00 PM")).toBe("Limit reached · resumes 2:00 PM");
    expect(activityText(stopped, stopped.name, null)).toBe("Limit reached");
    expect(phaseTint("limited")).toBe(LIMITED_TINT);
    expect(motionForPhase("limited")).toBe("doze");
  });

  it("is counted on the strip after the ones at work, and the limit's share widens it", () => {
    const counts = { working: 2, workingCodex: 1, waiting: 1, limited: 2, finished: 1 };
    const items = countItems(counts);
    expect(items[3]).toMatchObject({ n: 2, Icon: GaugeFill, tint: LIMITED_TINT });
    expect(readingWidth(counts, 12, "8%")).toBeGreaterThan(readingWidth(counts, 12) + 15);
    expect(readingWidth({ working: 0, workingCodex: 0, waiting: 0, finished: 0 }, 12, "8%")).toBeGreaterThan(0);
    expect(stripWing(counts, 24, "100%")).toBeGreaterThanOrEqual(stripWing(counts, 24));
    expect(countItems({ working: 1, workingCodex: 0, waiting: 0, finished: 0 })).toHaveLength(1);
  });
});
