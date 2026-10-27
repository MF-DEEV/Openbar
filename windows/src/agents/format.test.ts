// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  clock,
  cost,
  countdown,
  duration,
  LIMIT_ORANGE,
  LIMIT_RED,
  limitTint,
  modelName,
  percent,
  PROVIDER_TINTS,
  tokens,
  utcDay,
  normalizedModel,
  tokenTotal,
  cacheHitRate,
} from "./format";

describe("number formats (spec 03 §10.6)", () => {
  it("cost: two decimals below $100, none from $100, thousands from $10,000", () => {
    expect(cost(0)).toBe("$0.00");
    expect(cost(4.56)).toBe("$4.56");
    expect(cost(99.994)).toBe("$99.99");
    expect(cost(100)).toBe("$100");
    expect(cost(1234.5)).toBe("$1,235");
    expect(cost(9999)).toBe("$9,999");
    expect(cost(10_000)).toBe("$10K");
    expect(cost(12_345)).toBe("$12K");
    expect(cost(Number.NaN)).toBe("$0");
  });

  it("tokens: plain below 1000, then K/M/B with one decimal below ten, always rounded down", () => {
    expect(tokens(0)).toBe("0");
    expect(tokens(842)).toBe("842");
    expect(tokens(999)).toBe("999");
    expect(tokens(1000)).toBe("1K");
    expect(tokens(4_290)).toBe("4.2K");
    expect(tokens(9_999)).toBe("9.9K");
    expect(tokens(48_900)).toBe("48K");
    expect(tokens(1_180_000)).toBe("1.1M");
    expect(tokens(999_999)).toBe("999K");
    expect(tokens(3_700_000_000)).toBe("3.7B");
    expect(tokens(-5)).toBe("0");
  });

  it("percent: whole numbers, clamped", () => {
    expect(percent(0.424)).toBe("42%");
    expect(percent(1.5)).toBe("100%");
    expect(percent(-1)).toBe("0%");
  });

  it("durations: abbreviated, two units", () => {
    expect(duration(252)).toBe("4m 12s");
    expect(duration(3900)).toBe("1h 05m");
    expect(duration(2 * 86_400 + 3 * 3600 + 59)).toBe("2d 3h");
    expect(duration(45)).toBe("45s");
    expect(duration(3 * 86_400)).toBe("3d");
    expect(duration(1440 * 60, 1)).toBe("1d");
  });

  it("countdowns never show seconds and last at least a minute", () => {
    const now = 1_000_000;
    expect(countdown(now + 20_000, now)).toBe("1m");
    expect(countdown(now + (2 * 3600 + 13 * 60) * 1000, now)).toBe("2h 13m");
    expect(countdown(now + 12 * 60_000 + 30_000, now)).toBe("12m");
  });

  it("clock: m:ss, then h:mm:ss", () => {
    expect(clock(42)).toBe("0:42");
    expect(clock(725)).toBe("12:05");
    expect(clock(3725)).toBe("1:02:05");
  });

  it("reads the price list's day in UTC", () => {
    expect(utcDay("2026-10-09")).toBe("Oct 9, 2026");
  });
});

describe("limit tint", () => {
  it("is the agent's own color until 80 % used, orange from 80 %, red from 95 %", () => {
    expect(limitTint("claude", 0.79)).toBe(PROVIDER_TINTS.claude);
    expect(limitTint("claude", 0.8)).toBe(LIMIT_ORANGE);
    expect(limitTint("codex", 0.949)).toBe(LIMIT_ORANGE);
    expect(limitTint("codex", 0.95)).toBe(LIMIT_RED);
    expect(limitTint("opencode", 1)).toBe(LIMIT_RED);
  });
});

describe("model names", () => {
  it("reads Claude and GPT ids as their family names", () => {
    expect(modelName("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelName("anthropic/claude-sonnet-4.5")).toBe("Sonnet 4.5");
    expect(modelName("claude-sonnet-4-5-20250929")).toBe("Sonnet 4.5");
    expect(modelName("gpt-6-astra")).toBe("GPT-6 Astra");
    expect(modelName("gpt-5.6")).toBe("GPT-5.6");
    expect(modelName("gpt-5-2025-08-07")).toBe("GPT-5");
    expect(modelName("gemini-3-pro")).toBe("gemini-3-pro");
  });
});

describe("model ids and token sums", () => {
  it("normalizes provider prefixes, dotted versions and context tags", () => {
    expect(normalizedModel("  Anthropic/Claude-Sonnet-4.5 ")).toBe("claude-sonnet-4-5");
    expect(normalizedModel("us.anthropic.claude-opus-4-1-20250805-v1")).toBe("claude-opus-4-1-20250805-v1");
    expect(normalizedModel("claude-opus-4-6[1m]")).toBe("claude-opus-4-6");
    expect(normalizedModel("claude-haiku-4-5@20251001")).toBe("claude-haiku-4-5");
    expect(normalizedModel("openai/gpt-5.6")).toBe("gpt-5.6");
  });

  it("sums every token kind and reads the cache's share of the prompt", () => {
    const t = { input: 10, cacheWrite: 20, cacheRead: 70, output: 5, reasoning: 3 };
    expect(tokenTotal(t)).toBe(105);
    expect(cacheHitRate(t)).toBeCloseTo(0.7);
    expect(cacheHitRate({ input: 0, cacheWrite: 0, cacheRead: 0, output: 9, reasoning: 0 })).toBeNull();
  });
});
