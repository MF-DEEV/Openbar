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
import { applyHolds, latest, percent, shownLevel, useAudio } from "./store";
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
  useAudio.setState({ state: EMPTY_AUDIO, received: false, holds: {}, switchError: null });
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

  it("keeps a switch error with the device's direction", async () => {
    useAudio.getState().receive({ ...reading(0.5), inputs: [{ id: "mic", name: "Mic", isDefault: false }] });
    await useAudio.getState().setDefault("mic");
    expect(useAudio.getState().switchError).toEqual({ input: true, message: "Element not found" });
    await useAudio.getState().setDefault("speakers");
    expect(useAudio.getState().switchError).toEqual({ input: false, message: "Element not found" });
  });

  it("toggles Mic Mute to the opposite of what shows", async () => {
    useAudio.getState().toggleMicMute();
    await Promise.resolve();
    expect(calls).toEqual([["mic", true]]);
    expect(useAudio.getState().state.micMuted).toBe(true);
  });

  it("sends one call per field at a time, then only the newest", async () => {
    const sent: number[] = [];
    let release = () => {};
    const slow = (n: number) => () =>
      new Promise<void>((resolve) => {
        sent.push(n);
        release = resolve;
      });
    latest("k", slow(1));
    latest("k", slow(2));
    latest("k", slow(3));
    expect(sent).toEqual([1]);
    release();
    await new Promise((r) => setTimeout(r));
    expect(sent).toEqual([1, 3]);
    release();
    await new Promise((r) => setTimeout(r));
    latest("k", slow(4));
    expect(sent).toEqual([1, 3, 4]);
    release();
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
