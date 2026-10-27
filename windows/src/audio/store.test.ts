// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
const calls = vi.hoisted(() => [] as unknown[][]);
vi.mock("../shared/bridge", () => ({
  bridge: {
    audio: {
      setOutput: async (...args: unknown[]) => void calls.push(["output", ...args]),
      setInput: async (...args: unknown[]) => void calls.push(["input", ...args]),
      setApp: async (...args: unknown[]) => void calls.push(["app", ...args]),
      setDefault: async () => {
        throw new Error("Element not found");
      },
      micMute: async (on: boolean) => void calls.push(["mic", on]),
    },
  },
}));
import { applyHolds, percent, shownLevel, useAudio } from "./store";
import { EMPTY_AUDIO, type AudioState } from "./types";

const reading = (volume: number, muted = false): AudioState => ({
  ...EMPTY_AUDIO,
  supported: true,
  output: { volume, muted },
  apps: [{ key: "a", path: null, name: "A", volume: 1, muted: false, playing: true, system: false }],
});

afterEach(() => {
  vi.useRealTimers();
  calls.length = 0;
  useAudio.setState({ state: EMPTY_AUDIO, holds: {}, error: null });
});

describe("audio store", () => {
  it("shows a set level at once and keeps it against older readings", () => {
    vi.useFakeTimers();
    useAudio.getState().receive(reading(0.2, true));
    useAudio.getState().setOutput({ volume: 0.7 });
    expect(calls).toEqual([["output", 0.7, undefined]]);
    expect(useAudio.getState().state.output).toEqual({ volume: 0.7, muted: false });
    useAudio.getState().receive(reading(0.3, true));
    expect(useAudio.getState().state.output).toEqual({ volume: 0.7, muted: false });
    vi.advanceTimersByTime(1000);
    useAudio.getState().receive(reading(0.3, true));
    expect(useAudio.getState().state.output).toEqual({ volume: 0.3, muted: true });
  });

  it("clamps levels and holds app changes by key", () => {
    useAudio.getState().receive(reading(0.5));
    useAudio.getState().setApp("a", { volume: 1.4 });
    expect(calls).toEqual([["app", "a", 1, undefined]]);
    useAudio.getState().setApp("a", { muted: true });
    expect(useAudio.getState().state.apps[0]).toMatchObject({ volume: 1, muted: true });
  });

  it("keeps the switch error for the band", async () => {
    await useAudio.getState().setDefault("x");
    expect(useAudio.getState().error).toBe("Element not found");
  });

  it("toggles Mic Mute to the opposite of what shows", async () => {
    await useAudio.getState().toggleMicMute();
    expect(calls).toEqual([["mic", true]]);
    expect(useAudio.getState().state.micMuted).toBe(true);
  });

  it("drops expired holds", () => {
    const state = reading(0.4);
    expect(applyHolds(state, { "output.volume": { value: 0.9, until: 0 } }, 10).output.volume).toBe(0.4);
  });

  it("reads levels as the sliders show them", () => {
    expect(shownLevel({ volume: 0.5, muted: true })).toBe(0);
    expect(shownLevel({ volume: null, muted: null })).toBeNull();
    expect(percent(0.456)).toBe(46);
    expect(percent(Number.NaN)).toBe(0);
  });
});
