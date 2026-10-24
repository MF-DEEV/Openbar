// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { CLAUDE_TINT, CODEX_TINT, phaseTint } from "./format";
import { SparkleFill } from "./glyphs";
import { countItems } from "./SessionsStrip";

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
