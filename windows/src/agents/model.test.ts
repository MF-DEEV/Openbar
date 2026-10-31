// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { setRegionalSettings } from "../i18n";

import { beforeEach, describe, expect, it } from "vitest";

beforeEach(() => setRegionalSettings({ preferred: ["en-US"], region: "US", hourCycle: "h12", firstWeekday: 0 }));
import { pageHeight } from "./AgentsPage";
import {
  binding,
  cardWindows,
  contentHeight,
  current,
  focusedLimit,
  heatLevels,
  heatOpacity,
  limitShare,
  orderedCards,
  pace,
  readingShape,
  rowHeight,
  rows,
  shownCards,
  stripReading,
  tiles,
  weekColumns,
  windowLabel,
  limitsAreStale,
  CLAUDE_FRESHNESS,
  streak,
  accountNotes,
  ACCOUNT_STALENESS,
  pageProviders,
  showsCodexAccounts,
} from "./model";
import { A } from "./strings";
import { emptySnapshot, emptyTotals } from "./store";
import type { LimitWindow, Limits } from "./types";

const ids = (r: ReturnType<typeof rows>) => r.map((row) => row.map((t) => t.id));

describe("cards and rows (NotchAgentSupport)", () => {
  it("keeps the saved order and appends cards it leaves out", () => {
    expect(orderedCards(["trend", "limits", "trend"])).toEqual([
      "trend",
      "limits",
      "spend",
      "live",
      "models",
      "projects",
      "activity",
      "resets",
    ]);
    expect(shownCards([], ["models", "resets"])).not.toContain("models");
  });

  it("gives every agent that reports limits a card, and Resets only with Codex", () => {
    const list = tiles(["limits", "resets", "spend"], ["claude", "copilot"]);
    expect(list.map((t) => t.id)).toEqual(["limits.claude", "spend"]);
    expect(tiles(["resets"], ["codex"]).map((t) => t.id)).toEqual(["resets.codex"]);
  });

  it("pairs cards in reading order at 390 or wider; charts take a row", () => {
    const list = tiles(["limits", "spend", "trend", "live", "models", "activity", "projects"], ["claude", "codex"]);
    expect(ids(rows(list, 496))).toEqual([
      ["limits.claude", "limits.codex"],
      ["spend"],
      ["trend"],
      ["live", "models"],
      ["activity"],
      ["projects"],
    ]);
    expect(rows(list, 389).every((row) => row.length === 1)).toBe(true);
  });

  it("measures rows 96 tall, 118 for charts, 10 apart", () => {
    const list = rows(tiles(["limits", "trend"], ["claude", "codex"]), 496);
    expect(list.map(rowHeight)).toEqual([96, 118]);
    expect(contentHeight(list)).toBe(96 + 118 + 10);
    expect(contentHeight([])).toBe(0);
  });

  it("gives each added Codex home a card in Codex's place, and the Resets card names its account", () => {
    const list = tiles(["limits", "resets", "spend"], ["claude", "codex"], ["a", "b"]);
    expect(list.map((t) => t.id)).toEqual([
      "limits.claude",
      "limits.codex.a",
      "limits.codex.b",
      "resets.codex",
      "spend",
    ]);
    expect(list.filter((t) => t.account).map((t) => t.account)).toEqual(["a", "b"]);
    expect(list.find((t) => t.card === "resets")?.namesDefaultAccount).toBe(true);
    expect(tiles(["resets"], ["codex"])[0]?.namesDefaultAccount).toBe(false);
    // Without Codex read, the homes have no cards at all.
    expect(tiles(["limits", "resets"], ["claude"], ["a"]).map((t) => t.id)).toEqual(["limits.claude"]);
  });

  it("measures rows that hold an account card, or the Resets card beside them, 116 tall", () => {
    const list = rows(tiles(["limits", "resets", "spend", "trend"], ["claude", "codex"], ["a"]), 496);
    expect(ids(list)).toEqual([["limits.claude", "limits.codex.a"], ["resets.codex", "spend"], ["trend"]]);
    expect(list.map(rowHeight)).toEqual([116, 116, 118]);
    expect(rowHeight(rows(tiles(["spend", "live"], []), 496)[0]!)).toBe(96);
  });

  it("shows Codex for its added homes even before it left a log", () => {
    const snapshot = emptySnapshot();
    const enabled = { claude: true, codex: true, opencode: true, copilot: true };
    expect(pageProviders(snapshot, enabled)).toEqual([]);
    expect(pageProviders(snapshot, enabled, true)).toEqual(["codex"]);
    expect(pageProviders(snapshot, { ...enabled, codex: false }, true)).toEqual([]);
    expect(showsCodexAccounts(true, [])).toBe(true);
    expect(showsCodexAccounts(true, ["limits"])).toBe(false);
    expect(showsCodexAccounts(false, [])).toBe(false);
  });

  it("puts a failure in place of the windows or under them, never both", () => {
    expect(accountNotes(false, true, "Failed", null)).toEqual({ placeholder: A.loading, caption: null });
    expect(accountNotes(false, false, "Failed", null)).toEqual({ placeholder: "Failed", caption: null });
    expect(accountNotes(false, false, null, null)).toEqual({ placeholder: A.accountRefreshHint, caption: null });
    expect(accountNotes(true, false, "Failed", "Updated 1m")).toEqual({ placeholder: null, caption: "Failed" });
    expect(accountNotes(true, true, null, "Updated 1m")).toEqual({ placeholder: null, caption: "Updated 1m" });
  });

  it("asks the island for the cards' height up to the budget, 140 when empty", () => {
    expect(pageHeight(false, 2, 400, 264)).toBeNull();
    expect(pageHeight(true, 0, 0, 264)).toBe(140);
    expect(pageHeight(true, 2, 0, 264)).toBe(140);
    expect(pageHeight(true, 2, 224, 264)).toBe(224);
    expect(pageHeight(true, 2, 430, 264)).toBe(264);
  });
});

const window = (over: Partial<LimitWindow>): LimitWindow => ({
  id: "w",
  kind: "session",
  minutes: 300,
  scope: null,
  usedPercent: 40,
  resetsAt: null,
  estimatedResetsAt: null,
  ...over,
});
const limits = (windows: LimitWindow[]): Limits => ({ provider: "claude", windows, observedAt: 0, source: "account" });

describe("limit windows (AgentLimitSupport)", () => {
  const now = 10 * 3_600_000;

  it("a window whose reset passed has nothing spent", () => {
    expect(current(window({ resetsAt: now - 1 }), now).usedPercent).toBe(0);
    expect(current(window({ resetsAt: now + 1 }), now).usedPercent).toBe(40);
  });

  it("places the pace tick at the elapsed share of the window", () => {
    expect(pace(window({ resetsAt: now + 150 * 60_000 }), now)).toBeCloseTo(0.5);
    expect(pace(window({ resetsAt: null }), now)).toBeNull();
  });

  it("binds on the most spent window, the later reset on a tie", () => {
    const a = window({ id: "a", usedPercent: 60, resetsAt: now + 1000 });
    const b = window({ id: "b", kind: "weekly", usedPercent: 60, resetsAt: now + 5000 });
    expect(binding(limits([a, b]), now)?.id).toBe("b");
  });

  it("focuses the chosen plan-wide window, never a model's own", () => {
    const session = window({ id: "s", usedPercent: 10 });
    const opus = window({ id: "o", kind: "weekly", scope: "Opus", usedPercent: 90 });
    const week = window({ id: "w", kind: "weekly", usedPercent: 30 });
    expect(focusedLimit(limits([session, opus, week]), "weekly", now)?.id).toBe("w");
    expect(focusedLimit(limits([session, opus]), "weekly", now)?.id).toBe("o");
    expect(focusedLimit(limits([session, opus, week]), "mostUsed", now)?.id).toBe("o");
  });

  it("shows the session and the most spent longer window", () => {
    const list = cardWindows(
      limits([
        window({ id: "s" }),
        window({ id: "w", kind: "weekly", usedPercent: 20 }),
        window({ id: "o", kind: "weekly", scope: "Opus", usedPercent: 70 }),
      ]),
      now,
    );
    expect(list.map((w) => w.id)).toEqual(["s", "o"]);
  });

  it("names windows and reads their share", () => {
    expect(windowLabel(window({}))).toBe("Session");
    expect(windowLabel(window({ kind: "weekly", scope: "Opus" }))).toBe("Week · Opus");
    expect(windowLabel(window({ kind: "other", minutes: 1440 }))).toBe("1d");
    expect(windowLabel(window({ kind: "other", minutes: null }))).toBe("Limit");
    expect(limitShare(window({ usedPercent: 80 }), "remaining")).toBe("20% left");
    expect(limitShare(window({ usedPercent: 80 }), "used")).toBe("80% used");
  });
});

describe("strip reading", () => {
  const now = 1_000_000_000;
  const snapshot = emptySnapshot();
  snapshot.live = [
    {
      id: "a",
      provider: "codex",
      started: now - 754_000,
      lastActivity: now,
      model: "",
      project: "",
      tokens: { input: 1_180_000, cacheWrite: 0, cacheRead: 0, output: 20_000, reasoning: 0 },
      cost: 4.56,
    },
  ];

  it("reads elapsed time, tokens written, cost or the focused limit", () => {
    expect(stripReading(snapshot, "elapsed", "remaining", "mostUsed", now)).toBe("12:34");
    expect(stripReading(snapshot, "tokens", "remaining", "mostUsed", now)).toBe("20K");
    expect(stripReading(snapshot, "cost", "remaining", "mostUsed", now)).toBe("$4.56");
    // No limits known: the time elapsed stands in.
    expect(stripReading(snapshot, "limit", "remaining", "mostUsed", now)).toBe("12:34");
    snapshot.limits.codex = { ...limits([window({ usedPercent: 30 })]), provider: "codex" };
    expect(stripReading(snapshot, "limit", "remaining", "mostUsed", now)).toBe("70%");
    expect(stripReading(snapshot, "limit", "used", "mostUsed", now)).toBe("30%");
  });

  it("measures on the reading's shape", () => {
    expect(readingShape("12:34")).toBe(readingShape("59:59"));
    expect(readingShape("1:00:00")).not.toBe(readingShape("59:59"));
  });
});

describe("activity heatmap", () => {
  it("shades by quartile of the active days", () => {
    const levels = heatLevels([0, 1, 2, 3, 4, 0]);
    expect(levels).toEqual([2, 3, 4]);
    expect(heatOpacity(0, levels)).toBe(0.07);
    expect(heatOpacity(1, levels)).toBe(0.28);
    expect(heatOpacity(4, levels)).toBe(0.65);
    expect(heatOpacity(9, levels)).toBe(0.9);
  });

  it("lays days into weeks from the first weekday", () => {
    // 2026-10-19 is a Monday.
    const days = Array.from({ length: 10 }, (_, i) => ({ start: new Date(2026, 9, 19 + i).getTime() }));
    const weeks = weekColumns(days, 1);
    expect(weeks).toHaveLength(2);
    expect(weeks[0]![0]).toBe(days[0]);
    expect(weekColumns(days, 0)[0]![1]).toBe(days[0]);
  });
});

describe("activity streak and limit freshness", () => {
  const day = (requests: number) => ({
    start: 0,
    byProvider: requests ? { claude: { ...emptyTotals(), requests } } : {},
  });
  const withDays = (requests: number[]) => ({ ...emptySnapshot(), days: requests.map(day) });

  it("counts days in a row with use, today only once it has some", () => {
    expect(streak(withDays([1, 0, 3, 2, 5]))).toBe(3);
    expect(streak(withDays([1, 0, 3, 2, 0]))).toBe(2);
    expect(streak(withDays([4, 0, 0]))).toBe(0);
    expect(streak(withDays([]))).toBe(0);
  });

  it("calls only Claude's limits stale, after half an hour", () => {
    const limits = { observedAt: 1_000 } as Limits;
    expect(limitsAreStale(limits, "claude", 1_000 + CLAUDE_FRESHNESS - 1)).toBe(false);
    expect(limitsAreStale(limits, "claude", 1_000 + CLAUDE_FRESHNESS)).toBe(true);
    expect(limitsAreStale(limits, "codex", 1_000 + 10 * CLAUDE_FRESHNESS)).toBe(false);
    expect(limitsAreStale(undefined, "claude", 1e12)).toBe(false);
    // An account card's reading after two missed checks.
    expect(limitsAreStale(limits, "codex", 1_000 + ACCOUNT_STALENESS - 1, true)).toBe(false);
    expect(limitsAreStale(limits, "codex", 1_000 + ACCOUNT_STALENESS, true)).toBe(true);
    expect(limitsAreStale(null, "codex", 1e12, true)).toBe(false);
  });
});
