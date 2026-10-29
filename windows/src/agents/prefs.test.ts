// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../island/settings";
import { resolve } from "../shared/prefs";

describe("AI Agents preferences", () => {
  it("start as on the Mac", () => {
    const { island } = resolve(null);
    expect(island.sections.agents).toBe(true);
    expect([island.agentsClaude, island.agentsCodex, island.agentsOpenCode, island.agentsCopilot]).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect(island.agentsPeriod).toBe("today");
    expect(island.agentsLimitDisplay).toBe("remaining");
    expect(island.agentsLimitFocus).toBe("mostUsed");
    expect(island.agentsReadout).toBe("elapsed");
    expect(island.agentsFinishMinimum).toBe(60);
    expect(island.agentsLimitThreshold).toBe(80);
    expect(island.agentsDailyBudget).toBe(0);
    expect(island.agentsPriceUpdates).toBe(true);
  });

  it("read every agent switched off as all on, as the usage service does", () => {
    const { island } = resolve({
      island: { agentsClaude: false, agentsCodex: false, agentsOpenCode: false, agentsCopilot: false },
    });
    expect([island.agentsClaude, island.agentsCodex, island.agentsOpenCode, island.agentsCopilot]).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect(resolve({ island: { agentsClaude: false } }).island.agentsClaude).toBe(false);
  });

  it("read unknown choices as the default", () => {
    const { island } = resolve({
      island: {
        agentsPeriod: "year",
        agentsReadout: "speed",
        agentsLimitDisplay: "used",
        agentsLimitFocus: 3,
      } as never,
    });
    expect(island.agentsPeriod).toBe(DEFAULT_SETTINGS.agentsPeriod);
    expect(island.agentsReadout).toBe("elapsed");
    expect(island.agentsLimitDisplay).toBe("used");
    expect(island.agentsLimitFocus).toBe("mostUsed");
  });

  it("keep only known cards, each once", () => {
    const { island } = resolve({
      island: { agentsCardOrder: ["trend", "nope", "trend", 4, "limits"], agentsHiddenCards: "resets" } as never,
    });
    expect(island.agentsCardOrder).toEqual(["trend", "limits"]);
    expect(island.agentsHiddenCards).toEqual([]);
  });

  it("keep Codex homes only when whole and unique, at most eight", () => {
    const id = (n: number) => `0000000${n}-0000-4000-8000-000000000000`;
    const { island } = resolve({
      island: {
        agentsCodexProfiles: [
          { id: id(1), name: "Work", directory: "C:\\Users\\me\\.codex-work", extra: true },
          { id: id(1), name: "Same id", directory: "C:\\Users\\me\\.codex-x" },
          { id: id(2), name: "Same folder", directory: "c:/users/me/.codex-work/" },
          { id: "nope", name: "Bad id", directory: "C:\\a" },
          { id: id(3), name: "   ", directory: "C:\\b" },
          { id: id(4), name: "x".repeat(41), directory: "C:\\c" },
          { id: id(5), name: "Relative", directory: "codex" },
          { id: id(6), name: "Share", directory: "\\\\server\\codex" },
          "junk",
        ],
      } as never,
    });
    expect(island.agentsCodexProfiles).toEqual([
      { id: id(1), name: "Work", directory: "C:\\Users\\me\\.codex-work" },
      { id: id(6), name: "Share", directory: "\\\\server\\codex" },
    ]);
    expect(resolve({ island: { agentsCodexProfiles: "[]" } as never }).island.agentsCodexProfiles).toEqual([]);
    const many = Array.from({ length: 10 }, (_, n) => ({ id: id(n), name: `N${n}`, directory: `C:\\h${n}` }));
    expect(resolve({ island: { agentsCodexProfiles: many } }).island.agentsCodexProfiles).toHaveLength(8);
  });

  it("clamp numbers as the Mac reads them", () => {
    const { island } = resolve({
      island: { agentsFinishMinimum: 99_999, agentsLimitThreshold: 0, agentsDailyBudget: -5 },
    });
    expect(island.agentsFinishMinimum).toBe(3600);
    expect(island.agentsLimitThreshold).toBe(1);
    expect(island.agentsDailyBudget).toBe(0);
  });
});
