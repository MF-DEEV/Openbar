// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";

const watched = vi.hoisted(() => ({
  calls: [] as [boolean, string | null][],
  speedStarts: 0,
  speedHandler: null as ((state: SpeedTestState) => void) | null,
  speedStored: async (): Promise<SpeedTestState | undefined> => undefined,
}));
vi.mock("../shared/bridge", () => ({
  bridge: {
    system: {
      watch: async (visible: boolean, detail: string | null) => {
        watched.calls.push([visible, detail]);
      },
      snapshot: async () => undefined,
      onSnapshot: async () => () => {},
      speedTest: () => watched.speedStored(),
      startSpeedTest: async () => {
        watched.speedStarts++;
      },
      onSpeedTest: async (handler: (state: SpeedTestState) => void) => {
        watched.speedHandler = handler;
        return () => {};
      },
    },
  },
}));

import { DEFAULT_SETTINGS } from "../island/settings";
import { useIsland } from "../island/store";
import { demoSnapshot } from "./demo";
import { subscribeSystem } from "./register";
import { emptyHistory, HISTORY_LENGTH, idleSpeedTest, pushHistory, showNetworkReading, useSystem } from "./store";
import type { SpeedTestState } from "./types";

afterEach(() => {
  useIsland.getState().collapse();
  useIsland.setState({ settings: DEFAULT_SETTINGS });
  useSystem.setState({ snapshot: null, history: emptyHistory(), detail: null, speedTest: idleSpeedTest });
  watched.calls = [];
  watched.speedStarts = 0;
});

describe("system history", () => {
  it("adds each reading once and keeps the last sixty", () => {
    let history = emptyHistory();
    for (let seq = 1; seq <= HISTORY_LENGTH + 5; seq++) history = pushHistory(history, demoSnapshot(seq));
    expect(history.cpu).toHaveLength(HISTORY_LENGTH);
    expect(history.cpu.at(-1)).toBe(demoSnapshot(HISTORY_LENGTH + 5).cpu);
    expect(history.memory.at(-1)).toBeCloseTo(0.63, 1);
    expect(history.battery.at(-1)).toBe(0.18);
    expect(history.power.at(-1)).toBeGreaterThan(0);
  });

  it("skips a missing value instead of drawing a zero", () => {
    const history = pushHistory(pushHistory(emptyHistory(), demoSnapshot(1)), { ...demoSnapshot(2), gpu: null });
    expect(history.gpu).toHaveLength(1);
    expect(history.cpu).toHaveLength(2);
  });

  it("doesn't record the stored reading fetched on opening twice", () => {
    useSystem.getState().receive(demoSnapshot(7));
    useSystem.getState().receive(demoSnapshot(7));
    expect(useSystem.getState().history.cpu).toHaveLength(1);
    useSystem.getState().receive({ ...demoSnapshot(0), seq: 0 });
    expect(useSystem.getState().history.cpu).toHaveLength(1);
  });
});

describe("sampling follows the page", () => {
  it("runs only while the System page is open, with its detail", () => {
    useIsland.getState().open("system");
    expect(watched.calls.at(-1)).toEqual([true, null]);
    useSystem.getState().openDetail("cpu");
    expect(watched.calls.at(-1)).toEqual([true, "cpu"]);
    useIsland.getState().toggleSections();
    expect(watched.calls.at(-1)).toEqual([false, null]);
    // Leaving the page closed the detail, as collapsing does on the Mac.
    expect(useSystem.getState().detail).toBeNull();
    useIsland.getState().toggleSections();
    expect(watched.calls.at(-1)).toEqual([true, null]);
    useIsland.getState().select("music");
    expect(watched.calls.at(-1)).toEqual([false, null]);
  });

  it("stops when the island closes or the section is hidden", () => {
    useIsland.getState().open("system");
    useIsland.getState().collapse();
    expect(watched.calls.at(-1)).toEqual([false, null]);
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, hiddenModules: ["system"] } });
    useIsland.getState().open("system");
    expect(watched.calls.at(-1)).toEqual([false, null]);
  });

  it("sends a change once", () => {
    useIsland.getState().open("system");
    const count = watched.calls.length;
    useIsland.getState().pointerEnter();
    useIsland.getState().pointerExit();
    expect(watched.calls.length).toBe(count);
  });
});

describe("the Controls speed test tile", () => {
  it("lands on System's network reading", () => {
    useIsland.getState().open("controls");
    showNetworkReading();
    expect(useIsland.getState().selected).toBe("system");
    expect(useSystem.getState().detail).toBe("network");
  });
});

describe("the speed test", () => {
  it("asks the app to start one only when none is running", () => {
    useSystem.getState().startSpeedTest();
    expect(watched.speedStarts).toBe(1);
    for (const phase of ["latency", "download", "upload"] as const) {
      useSystem.getState().receiveSpeedTest({ ...idleSpeedTest, phase });
      useSystem.getState().startSpeedTest();
    }
    expect(watched.speedStarts).toBe(1);
    useSystem.getState().receiveSpeedTest({ ...idleSpeedTest, phase: "done", downloadMbps: 1, uploadMbps: 1 });
    useSystem.getState().startSpeedTest();
    useSystem.getState().receiveSpeedTest({ ...idleSpeedTest, phase: "failed" });
    useSystem.getState().startSpeedTest();
    expect(watched.speedStarts).toBe(3);
  });

  it("keeps its result when the detail closes", () => {
    useIsland.getState().open("system");
    useSystem.getState().openDetail("network");
    const done = { ...idleSpeedTest, phase: "done" as const, latencyMs: 9, downloadMbps: 300, uploadMbps: 40 };
    useSystem.getState().receiveSpeedTest(done);
    useIsland.getState().collapse();
    expect(useSystem.getState().detail).toBe(null);
    expect(useSystem.getState().speedTest).toEqual(done);
  });

  it("keeps a state that came while the stored one was fetched", async () => {
    let answer: ((state: SpeedTestState) => void) | undefined;
    watched.speedStored = () => new Promise((resolve) => (answer = resolve));
    const subscribing = subscribeSystem();
    await vi.waitFor(() => expect(answer).toBeDefined());
    const done = { ...idleSpeedTest, phase: "done" as const, downloadMbps: 300, uploadMbps: 40 };
    watched.speedHandler!(done);
    answer!({ ...idleSpeedTest, phase: "upload" });
    await subscribing;
    expect(useSystem.getState().speedTest).toEqual(done);
    watched.speedStored = async () => undefined;
  });
});
