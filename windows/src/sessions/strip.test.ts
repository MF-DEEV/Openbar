// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { CLAUDE_TINT, CODEX_TINT, phaseTint } from "./format";
import { SparkleFill } from "./glyphs";
import {
  countItems,
  marksWidth,
  readingWidth,
  STACK_OFFSET,
  stripAgents,
  stripWing,
  WIDEST_WING,
} from "./SessionsStrip";

describe("counts by agent", () => {
  it("splits working sessions into Claude and Codex, each in its own color", () => {
    const items = countItems({ working: 3, workingCodex: 1, waiting: 0, finished: 2 });
    expect(items.map((i) => [i.Icon, i.n, i.tint])).toEqual([
      [SparkleFill, 2, CLAUDE_TINT],
      [SparkleFill, 1, CODEX_TINT],
      [items[2]!.Icon, 2, items[2]!.tint],
    ]);
  });

  it("leaves out an agent with nothing running", () => {
    const items = countItems({ working: 1, workingCodex: 1, waiting: 0, finished: 0 });
    expect(items.map((i) => i.tint)).toEqual([CODEX_TINT]);
  });

  it("tints working phases by agent and keeps the shared ones", () => {
    expect(phaseTint("generating", "codex")).toBe(CODEX_TINT);
    expect(phaseTint("runningTool", "claude")).toBe(CLAUDE_TINT);
    expect(phaseTint("finished", "codex")).toBe(phaseTint("finished", "claude"));
  });
});

describe("stacked marks", () => {
  const counts = (working: number, workingCodex: number, waiting = 0, finished = 0) => ({
    working,
    workingCodex,
    waiting,
    finished,
  });

  it("stacks both marks, Claude's in front, only while both agents work", () => {
    expect(stripAgents(counts(3, 1))).toEqual(["claude", "codex"]);
    expect(stripAgents(counts(1, 1))).toEqual(["codex"]);
    expect(stripAgents(counts(2, 0))).toEqual(["claude"]);
    expect(stripAgents(counts(0, 0, 1, 2))).toEqual([]);
  });

  it("never shows an agent with nothing at work for counts that make no sense", () => {
    expect(stripAgents(counts(1, 4))).toEqual(["codex"]);
    expect(stripAgents(counts(-1, -3))).toEqual([]);
    expect(stripAgents(counts(2, -1))).toEqual(["claude"]);
  });

  it("makes room for the mark behind only while both marks show", () => {
    const lone = marksWidth(counts(2, 0));
    expect(marksWidth(counts(2, 1))).toBeGreaterThan(lone + STACK_OFFSET / 2);
    expect(marksWidth(counts(2, 1, 1))).toBe(lone);
    expect(marksWidth(counts(0, 0, 0, 1))).toBe(lone);
    expect(stripWing(counts(2, 1), 24)).toBeGreaterThanOrEqual(stripWing(counts(2, 0), 24));
  });
});

describe("busy strip", () => {
  // Six counts at once, as a busy fleet shows them: Claude, Codex, Gemini,
  // Copilot, OpenCode at work and one finished.
  const busy = {
    working: 13,
    workingCodex: 3,
    workingGemini: 2,
    workingCopilot: 1,
    workingOpencode: 2,
    waiting: 0,
    finished: 1,
  };

  it("gives six counts the wing they need instead of the agents' strip's 80", () => {
    expect(countItems(busy)).toHaveLength(6);
    const wing = stripWing(busy, 32);
    expect(wing).toBeGreaterThan(80);
    expect(wing).toBeLessThanOrEqual(WIDEST_WING);
    expect(wing).toBeGreaterThanOrEqual(Math.ceil(readingWidth(busy, 15)));
  });

  it("stops at the widest wing", () => {
    const many = { ...busy, working: 9_999_999, waiting: 9_999_999, limited: 9_999_999, finished: 9_999_999 };
    expect(stripWing(many, 32, "100%")).toBe(WIDEST_WING);
  });
});
