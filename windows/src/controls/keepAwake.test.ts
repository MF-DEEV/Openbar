// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
const keepAwake = vi.hoisted(() => ({ state: { active: true, endsAt: null as number | null } }));
vi.mock("../shared/bridge", () => ({
  bridge: {
    keepAwake: {
      toggle: async () => keepAwake.state,
      onState: async () => () => {},
      state: async () => keepAwake.state,
    },
  },
}));
import { keepAwakeText, minutesLeft, nextTick, toggleKeepAwake, useKeepAwake } from "./keepAwake";
import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";

afterEach(() => {
  useIsland.setState({ settings: DEFAULT_SETTINGS, activities: {} });
  useKeepAwake.setState({ state: { active: false, endsAt: null } });
});

describe("keep awake reading", () => {
  const now = 1_000_000;
  it("rounds minutes up, never below one", () => {
    expect(minutesLeft(now + 30 * 60_000, now)).toBe(30);
    expect(minutesLeft(now + 29 * 60_000 + 1, now)).toBe(30);
    expect(minutesLeft(now + 1_000, now)).toBe(1);
    expect(minutesLeft(now - 5_000, now)).toBe(1);
  });

  it("reads like the timer strip: 45m, then 1h35 and past three hours", () => {
    expect(keepAwakeText(now + 45 * 60_000, now)).toBe("45m");
    expect(keepAwakeText(now + 95 * 60_000, now)).toBe("1h35");
    expect(keepAwakeText(now + 60 * 60_000, now)).toBe("1h00");
    expect(keepAwakeText(now + 480 * 60_000, now)).toBe("8h00");
  });

  it("ticks on the remaining time's minute boundary", () => {
    expect(nextTick(now + 90_000, now)).toBe(30_000);
    expect(nextTick(now + 120_000, now)).toBe(60_000);
  });
});

describe("keep awake strip", () => {
  it("takes the closed island only when the person turned it on", async () => {
    await toggleKeepAwake();
    expect(useIsland.getState().activities.keepAwake).toBeUndefined();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, keepAwakeActivity: true } });
    expect(useIsland.getState().activities.keepAwake).toMatchObject({ opens: "controls" });
    useKeepAwake.setState({ state: { active: false, endsAt: null } });
    expect(useIsland.getState().activities.keepAwake).toBeUndefined();
  });
});
