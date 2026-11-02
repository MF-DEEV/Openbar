// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { characterOf, FAVOURED_SHELL } from "./clawd/costume";
import { CLAUDE_TINT, GEMINI_TINT, OPENCODE_TINT, providerTint } from "./format";
import { MARK_SCALE, markOf } from "./glyphs";
import { countItems, stripAgents } from "./SessionsStrip";
import { S } from "./strings";
import { HOOK_AGENTS, isHookAgent, PROVIDERS, workingOf, type Counts } from "./types";

const counts = (working: number, others: Partial<Counts> = {}): Counts => ({
  working,
  workingCodex: 0,
  waiting: 0,
  finished: 0,
  ...others,
});

describe("Gemini CLI, Copilot CLI, OpenCode and Cursor in Sessions", () => {
  it("counts each agent's work apart, Claude's being what no other claims", () => {
    const c = counts(4, { workingGemini: 1, workingOpencode: 2 });
    expect(PROVIDERS.map((p) => workingOf(c, p))).toEqual([1, 0, 1, 0, 2, 0]);
    expect(workingOf(counts(1, { workingCursor: 5 }), "cursor")).toBe(1);
    expect(workingOf(counts(-1, { workingGemini: -3 }), "gemini")).toBe(0);
  });

  it("gives each agent its own count in its color, and stacks the first two marks", () => {
    const c = counts(3, { workingGemini: 1, workingOpencode: 1 });
    expect(countItems(c).map((i) => i.tint)).toEqual([CLAUDE_TINT, GEMINI_TINT, OPENCODE_TINT]);
    expect(stripAgents(c)).toEqual(["claude", "gemini"]);
    expect(stripAgents(counts(1, { workingCursor: 1 }))).toEqual(["cursor"]);
  });

  it("draws every agent as itself", () => {
    expect(new Set(PROVIDERS.map(characterOf)).size).toBe(PROVIDERS.length);
    expect(new Set(PROVIDERS.map(markOf)).size).toBe(PROVIDERS.length);
    expect(new Set(PROVIDERS.map(providerTint)).size).toBe(PROVIDERS.length);
    for (const p of PROVIDERS) {
      expect(MARK_SCALE[p]).toBeGreaterThan(1);
      expect(FAVOURED_SHELL[characterOf(p)]).toBeTruthy();
    }
    expect(characterOf("claude")).toBe("clawd");
    expect(characterOf("unknown")).toBe("clawd");
  });

  it("sends the person to the agent for what the island can't answer", () => {
    expect(HOOK_AGENTS.every(isHookAgent) && !isHookAgent("codex")).toBe(true);
    expect(S.answerIn("gemini")).toBe("Answer this one in Gemini CLI.");
    expect(S.answerIn("codex")).toBe(S.answerInTerminal);
    expect(S.replyPlaceholderFor("copilot")).toBe("Reply to Copilot CLI…");
    expect(S.replyPlaceholderFor("codex")).toBe(S.replyPlaceholderCodex);
    expect(S.replyIn("cursor")).toBe("Reply in Cursor");
  });
});
