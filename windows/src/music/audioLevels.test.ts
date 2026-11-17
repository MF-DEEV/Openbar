// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
vi.mock("../shared/bridge", () => ({ bridge: { music: { onLevels: vi.fn(), watchLevels: vi.fn() } } }));
import { bridge } from "../shared/bridge";
import { barHeights, subscribeAudioLevels, type AudioLevels } from "./audioLevels";

let receive: (frame: AudioLevels) => void;
let unlisten: ReturnType<typeof vi.fn<() => void>>;
const cleanups: (() => void)[] = [];
const flush = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  unlisten = vi.fn();
  vi.mocked(bridge.music.onLevels).mockImplementation(async (callback) => {
    receive = callback;
    return unlisten;
  });
});
afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  await flush();
  vi.useRealTimers();
});

it("samples the frequency range evenly and preserves bar bounds", () => {
  expect(barHeights(3, 2, 12, [0, 0.1, 0.2, 0.3, 0.4, 0.5, 1])).toEqual([5, 3, 7]);
  expect(barHeights(3, 2, 12, [])).toEqual([2, 2, 2]);
  expect(barHeights(4, 2, 12, [NaN, -1, 2, Infinity])).toEqual([12, 2, 2, 2]);
});

it("spreads the real bands in a stable order with bass away from the left edge", () => {
  expect(barHeights(7, 0, 1, [0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6])).toEqual([0.4, 0.1, 0.5, 0, 0.3, 0.6, 0.2]);
  expect(barHeights(7, 2, 12, [1, 0, 0, 0, 0, 0, 0])).toEqual([2, 2, 2, 12, 2, 2, 2]);
});

it("shares one listener and releases capture only after the last consumer", async () => {
  const a = vi.fn(),
    b = vi.fn();
  const offA = subscribeAudioLevels(a),
    offB = subscribeAudioLevels(b);
  cleanups.push(offA, offB);
  await flush();
  expect(bridge.music.onLevels).toHaveBeenCalledTimes(1);
  expect(bridge.music.watchLevels).toHaveBeenLastCalledWith(true);
  const frame: AudioLevels = { source: "Spotify.exe", mode: "process", levels: [0.4] };
  receive(frame);
  expect(a).toHaveBeenCalledWith(frame);
  expect(b).toHaveBeenCalledWith(frame);
  offA();
  await flush();
  expect(unlisten).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(5000);
  expect(bridge.music.watchLevels).toHaveBeenCalledTimes(2);
  offB();
  await flush();
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(bridge.music.watchLevels).toHaveBeenLastCalledWith(false);
  receive(frame);
  expect(b).toHaveBeenCalledTimes(2); // frame, then stale-data reset
});

it("settles when events stop arriving", async () => {
  const subscriber = vi.fn();
  cleanups.push(subscribeAudioLevels(subscriber));
  await flush();
  receive({ source: "player", mode: "system", levels: [1] });
  await vi.advanceTimersByTimeAsync(301);
  expect(subscriber).toHaveBeenLastCalledWith(null);
});

it("cleans up a listener that resolves after unmount without starting capture", async () => {
  let resolve!: (off: () => void) => void;
  vi.mocked(bridge.music.onLevels).mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const off = subscribeAudioLevels(vi.fn());
  off();
  resolve(unlisten);
  await flush();
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(bridge.music.watchLevels).not.toHaveBeenCalledWith(true);
});

it("does not start capture if listener registration fails", async () => {
  vi.mocked(bridge.music.onLevels).mockRejectedValue(new Error("unavailable"));
  const subscriber = vi.fn();
  cleanups.push(subscribeAudioLevels(subscriber));
  await flush();
  await vi.advanceTimersByTimeAsync(10000);
  expect(subscriber).toHaveBeenCalledWith(null);
  expect(bridge.music.watchLevels).not.toHaveBeenCalledWith(true);
});
