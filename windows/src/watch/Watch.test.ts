// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WatchFinished, WatchSnapshot } from "./types";

const connection = vi.hoisted(() => ({
  state: (_: WatchSnapshot) => {},
  finished: (_: WatchFinished) => {},
  calls: [] as unknown[][],
  chooses: true,
}));
vi.mock("../shared/bridge", () => ({
  inTauri: false,
  bridge: {
    watch: {
      snapshot: async () => undefined,
      onState: async (handler: (s: WatchSnapshot) => void) => {
        connection.state = handler;
        return () => {};
      },
      onFinished: async (handler: (e: WatchFinished) => void) => {
        connection.finished = handler;
        return () => {};
      },
      choose: async () => {
        connection.calls.push(["choose", useIsland.getState().expanded]);
        return connection.chooses;
      },
      stop: async () => void connection.calls.push(["stop"]),
      again: async () => void connection.calls.push(["again"]),
      setRule: async (rule: unknown) => void connection.calls.push(["setRule", rule]),
      pinLine: async (line: number | null) => void connection.calls.push(["pinLine", line]),
      pageVisible: async (visible: boolean) => void connection.calls.push(["pageVisible", visible]),
      notify: async (title: string, body: string) => {
        connection.calls.push(["notify", title, body]);
        return true;
      },
    },
  },
}));

import { DEFAULT_SETTINGS } from "../island/settings";
import { useIsland, visibleActivity } from "../island/store";
import { loadPrefs } from "../shared/prefs";
import "./register";
import { showsThumbnail, useWatch, watch } from "./store";
import { outcomeTitle, S } from "./strings";
import { IDLE } from "./types";
import { formatTarget, watchedTitle } from "./WatchPage";
import { breath, capsuleContent, stripWing, swing, THUMBNAIL_WIDTH } from "./WatchStrip";

const watching: WatchSnapshot = {
  ...IDLE,
  phase: "watching",
  target: { appName: "Notepad", windowTitle: "notes.txt - Notepad", icon: null, isWindow: true },
  preview: "data:image/png;base64,AAAA",
  text: "Build 42%\nok",
  headline: "42%",
  lines: ["Build 42%", "ok"],
  startedAt: 0,
};

beforeEach(() => {
  connection.calls.length = 0;
  connection.chooses = true;
});
afterEach(() => {
  vi.useRealTimers();
  useIsland.getState().dismissNotice();
  useIsland.setState({ settings: DEFAULT_SETTINGS, activities: {}, expanded: false, attentionFullscreen: false });
  useWatch.getState().receive(IDLE);
});

describe("closed island", () => {
  it("shows the strip while watching or hidden, never idle or finished", () => {
    connection.state(watching);
    expect(useIsland.getState().activities.watch?.opens).toBe("watch");
    connection.state({ ...watching, phase: "hidden" });
    expect(useIsland.getState().activities.watch).toBeDefined();
    connection.state({ ...watching, phase: "finished", outcome: { kind: "changed" } });
    expect(useIsland.getState().activities.watch).toBeUndefined();
    connection.state(watching);
    connection.state(IDLE);
    expect(useIsland.getState().activities.watch).toBeUndefined();
  });

  it("leaves the island when the section is switched off", () => {
    connection.state(watching);
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, hiddenModules: ["watch"] } });
    expect(useIsland.getState().activities.watch).toBeUndefined();
  });

  it("is second only to the timer", () => {
    connection.state(watching);
    const island = useIsland.getState();
    island.setActivity("music", { wing: 60, opens: "music" });
    island.setActivity("downloads", { wing: 60, opens: "downloads" });
    expect(visibleActivity(useIsland.getState())).toBe("watch");
    island.setActivity("timer", { wing: 60, opens: "timer" });
    expect(visibleActivity(useIsland.getState())).toBe("timer");
  });

  it("keeps its wings within 56…150 and gives an area without text 40 pt", () => {
    expect(stripWing({ ...watching, headline: "", preview: null }, 32)).toBe(56);
    expect(stripWing({ ...watching, headline: "x".repeat(80) }, 32)).toBe(150);
    expect(showsThumbnail({ ...watching, headline: "" })).toBe(true);
    expect(showsThumbnail({ ...watching, headline: "", preview: null })).toBe(false);
    expect(capsuleContent({ ...watching, headline: "" })).toBe(16 + 6 + THUMBNAIL_WIDTH);
    expect(capsuleContent({ ...watching, headline: "", preview: null })).toBe(16 + 6 + 16);
    expect(capsuleContent({ ...watching, headline: "x".repeat(200) })).toBe(220 - 16);
  });
});

describe("eye", () => {
  it("swings with a cosine ease over 1.2 s half cycles", () => {
    expect(swing(0)).toBe(0);
    expect(swing(1)).toBeCloseTo(1);
    expect(swing(0.5)).toBeCloseTo(0.5);
    expect(swing(2)).toBeCloseTo(0);
    expect(swing(-0.5)).toBeCloseTo(swing(1.5));
    expect(breath(0)).toEqual({ scale: 0.84, opacity: 0.7 });
    const peak = breath(1.2);
    expect(peak.scale).toBeCloseTo(1);
    expect(peak.opacity).toBeCloseTo(1);
  });
});

describe("ending", () => {
  const finished: WatchFinished = {
    outcome: { kind: "shows", text: "Done" },
    appName: "Notepad",
    windowTitle: "notes",
  };

  it("raises a priority-2 watch notice with the outcome and the app", () => {
    connection.finished(finished);
    const notice = useIsland.getState().notice;
    expect(notice).toMatchObject({
      kind: "watch",
      title: "It shows Done",
      detail: "Notepad",
      priority: 2,
      duration: 6,
    });
    expect(connection.calls.filter((c) => c[0] === "notify")).toEqual([]);
  });

  it("names a part of the display Watch", () => {
    connection.finished({ outcome: { kind: "changed" }, appName: "", windowTitle: null });
    expect(useIsland.getState().notice?.detail).toBe(S.title);
  });

  it("stays quiet when its page is open", () => {
    useIsland.setState({ expanded: true, selected: "watch" });
    connection.finished(finished);
    expect(useIsland.getState().notice).toBeNull();
    expect(connection.calls).toEqual([]);
  });

  it("posts a Windows notification when the island hides over a full-screen app", () => {
    useIsland.setState({ attentionFullscreen: true });
    connection.finished(finished);
    expect(useIsland.getState().notice).toBeNull();
    expect(connection.calls).toEqual([["notify", "It shows Done", "notes"]]);
  });

  it("posts one when a higher notice holds the island", () => {
    useIsland.getState().showNotice({ id: "level", kind: "volume", title: "50%", priority: 3, duration: 3 });
    connection.finished({ ...finished, windowTitle: null });
    expect(connection.calls).toEqual([["notify", "It shows Done", "Notepad"]]);
  });

  it("words every outcome as the Mac does", () => {
    expect(outcomeTitle({ kind: "changed" })).toBe("It changed");
    expect(outcomeTitle({ kind: "settled" })).toBe("It stopped changing");
    expect(outcomeTitle({ kind: "reached", text: "100%" })).toBe("It reached 100%");
    expect(outcomeTitle({ kind: "closed" })).toBe("The window closed");
  });
});

describe("page actions", () => {
  it("collapses the island before the chooser and opens Watch once a watch starts", async () => {
    vi.useFakeTimers();
    useIsland.setState({ expanded: true, selected: "watch" });
    const chosen = watch.choose();
    await vi.advanceTimersByTimeAsync(400);
    await chosen;
    expect(connection.calls[0]).toEqual(["choose", false]);
    expect(useIsland.getState().expanded).toBe(true);
    expect(useIsland.getState().selected).toBe("watch");
  });

  it("stays closed when the person cancels", async () => {
    connection.chooses = false;
    await watch.choose();
    expect(useIsland.getState().expanded).toBe(false);
  });

  it("saves the condition for the next watch and sends it", async () => {
    watch.setCondition("settles");
    await Promise.resolve();
    expect(connection.calls).toContainEqual(["setRule", { condition: "settles" }]);
    expect((await loadPrefs()).island.watchCondition).toBe("settles");
    watch.setCondition("changes");
  });

  it("names what is watched and shows a typed target without grouping", () => {
    expect(watchedTitle(watching)).toBe("Notepad · notes.txt - Notepad");
    expect(watchedTitle({ ...watching, target: { ...watching.target!, windowTitle: "Notepad" } })).toBe("Notepad");
    expect(watchedTitle({ ...watching, target: { ...watching.target!, appName: "", windowTitle: null } })).toBe(
      "Watch",
    );
    expect(formatTarget(null)).toBe("");
    expect(formatTarget(12500)).not.toMatch(/[ ,.  ]/);
  });
});
