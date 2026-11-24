// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { characterOf, FAVOURED_SHELL } from "./clawd/costume";
import {
  ANTIGRAVITY_TINT,
  HERMES_TINT,
  CLAUDE_TINT,
  GEMINI_TINT,
  GROK_TINT,
  OPENCODE_TINT,
  providerTint,
  ZCODE_TINT,
} from "./format";
import { MARK_SCALE, markOf } from "./glyphs";
import { countItems, stripAgents } from "./SessionsStrip";
import { S } from "./strings";
import { HOOK_AGENTS, isHookAgent, PROVIDER_NAMES, PROVIDERS, SHORT_NAMES, workingOf, type Counts } from "./types";

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
    expect(PROVIDERS.map((p) => workingOf(c, p))).toEqual([1, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
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
    expect(S.previewsOffFor("claude")).toBe(S.previewsOff);
    expect(S.previewsOffFor("codex")).toBe(S.previewsOffCodex);
    expect(S.previewsOffFor("gemini")).toBe("Turn on previews in Settings to see what Gemini CLI wrote.");
    expect(S.previewsOffFor("cursor")).not.toContain("Claude");
    // Under a question's card: the agent the answer goes to.
    expect(S.questionHintFor("claude")).toBe(S.questionHint);
    expect(S.questionHintFor("codex")).toBe(S.codexQuestionHint);
    expect(S.questionHintFor("hermes")).toBe(
      "Your answer goes straight to Hermes. Enter sends what you wrote, and Ctrl+N skips the question.",
    );
  });
});

describe("Pi, Mistral Vibe and Devin CLI in Sessions (#220)", () => {
  it("counts, names and draws each of the three as itself", () => {
    const c = counts(5, { workingPi: 1, workingVibe: 2, workingDevin: 1 });
    expect(["pi", "vibe", "devin"].map((p) => workingOf(c, p as never))).toEqual([1, 2, 1]);
    expect(workingOf(c, "claude")).toBe(1);
    expect(HOOK_AGENTS.slice(4, 7)).toEqual(["pi", "vibe", "devin"]);
    expect(stripAgents(counts(2, { workingVibe: 1, workingDevin: 1 }))).toEqual(["vibe", "devin"]);
    expect(characterOf("pi")).toBe("pi");
    expect(characterOf("devin")).toBe("devin");
  });

  it("says where each one's requests and replies go", () => {
    expect(S.answerIn("pi")).toBe("Answer this one in Pi.");
    expect(S.answerIn("vibe")).toBe("Answer this one in Mistral Vibe.");
    expect(S.answerIn("devin")).toBe("Answer this one in Devin CLI.");
    expect(S.replyPlaceholderFor("vibe")).toBe("Reply to Mistral Vibe…");
  });
});

describe("Google Antigravity in Sessions (#247)", () => {
  it("counts, names and draws it as itself", () => {
    const c = counts(3, { workingAntigravity: 2 });
    expect(workingOf(c, "antigravity")).toBe(2);
    expect(workingOf(c, "claude")).toBe(1);
    expect(HOOK_AGENTS.at(-4)).toBe("antigravity");
    expect(PROVIDER_NAMES.antigravity).toBe("Google Antigravity");
    expect(SHORT_NAMES.antigravity).toBe("Antigravity");
    expect(stripAgents(counts(2, { workingAntigravity: 1, workingDevin: 1 }))).toEqual(["devin", "antigravity"]);
    expect(characterOf("antigravity")).toBe("antigravity");
    expect(providerTint("antigravity")).toBe(ANTIGRAVITY_TINT);
  });

  it("sends its requests and replies to Antigravity", () => {
    expect(S.answerIn("antigravity")).toBe("Answer this one in Google Antigravity.");
    expect(S.replyIn("antigravity")).toBe("Reply in Google Antigravity");
  });
});

describe("Grok CLI in Sessions (#243)", () => {
  it("counts, names and draws it as itself", () => {
    const c = counts(3, { workingGrok: 2 });
    expect(workingOf(c, "grok")).toBe(2);
    expect(workingOf(c, "claude")).toBe(1);
    expect(HOOK_AGENTS).toContain("grok");
    expect(isHookAgent("grok")).toBe(true);
    expect(PROVIDER_NAMES.grok).toBe("Grok CLI");
    expect(SHORT_NAMES.grok).toBe("Grok");
    expect(stripAgents(counts(2, { workingAntigravity: 1, workingGrok: 1 }))).toEqual(["antigravity", "grok"]);
    expect(characterOf("grok")).toBe("grok");
    expect(FAVOURED_SHELL.grok).toBe("moss");
    expect(providerTint("grok")).toBe(GROK_TINT);
    expect(MARK_SCALE.grok).toBe(1.15);
    expect(markOf("grok")).not.toBe(markOf("claude"));
  });

  it("sends what the island can't answer to Grok CLI", () => {
    expect(S.answerIn("grok")).toBe("Answer this one in Grok CLI.");
    expect(S.replyPlaceholderFor("grok")).toBe("Reply to Grok CLI…");
  });
});

describe("ZCode in Sessions (#245)", () => {
  it("counts, names and draws it as itself", () => {
    const c = counts(3, { workingZcode: 2 });
    expect(workingOf(c, "zcode")).toBe(2);
    expect(workingOf(c, "claude")).toBe(1);
    expect(HOOK_AGENTS.at(-2)).toBe("zcode");
    expect(PROVIDERS.at(-2)).toBe("zcode");
    expect(isHookAgent("zcode")).toBe(true);
    expect(PROVIDER_NAMES.zcode).toBe("ZCode");
    expect(SHORT_NAMES.zcode).toBe("ZCode");
    expect(stripAgents(counts(2, { workingGrok: 1, workingZcode: 1 }))).toEqual(["grok", "zcode"]);
    expect(characterOf("zcode")).toBe("zcode");
    expect(FAVOURED_SHELL.zcode).toBe("rose");
    expect(providerTint("zcode")).toBe(ZCODE_TINT);
    expect(MARK_SCALE.zcode).toBe(1.1);
    expect(markOf("zcode")).not.toBe(markOf("grok"));
  });

  it("sends what the island can't answer to ZCode", () => {
    expect(S.answerIn("zcode")).toBe("Answer this one in ZCode.");
    expect(S.replyPlaceholderFor("zcode")).toBe("Reply to ZCode…");
  });
});

describe("Hermes in Sessions (#244)", () => {
  it("counts, names and draws it as itself", () => {
    const c = counts(3, { workingHermes: 2 });
    expect(workingOf(c, "hermes")).toBe(2);
    expect(workingOf(c, "claude")).toBe(1);
    expect(HOOK_AGENTS.at(-1)).toBe("hermes");
    expect(PROVIDERS.at(-1)).toBe("hermes");
    expect(isHookAgent("hermes")).toBe(true);
    expect(PROVIDER_NAMES.hermes).toBe("Hermes");
    expect(SHORT_NAMES.hermes).toBe("Hermes");
    expect(stripAgents(counts(2, { workingZcode: 1, workingHermes: 1 }))).toEqual(["zcode", "hermes"]);
    expect(characterOf("hermes")).toBe("hermes");
    expect(FAVOURED_SHELL.hermes).toBe("amber");
    expect(providerTint("hermes")).toBe(HERMES_TINT);
    expect(markOf("hermes")).not.toBe(markOf("zcode"));
  });

  it("sends what the island can't answer to Hermes", () => {
    expect(S.answerIn("hermes")).toBe("Answer this one in Hermes.");
    expect(S.replyPlaceholderFor("hermes")).toBe("Reply to Hermes…");
  });
});
