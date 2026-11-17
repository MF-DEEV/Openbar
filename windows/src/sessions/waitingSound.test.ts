// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { describe, expect, it } from "vitest";
import type { Session } from "./types";
import { inView, shouldPlay, waitingRequests, WaitingSound } from "./waitingSound";

function session(id: string, phase: Session["phase"], extra: Partial<Session> = {}): Session {
  return { id, phase, phaseSince: 100, provider: "claude", ...extra } as Session;
}

const permission = (requestId: string) =>
  ({ requestId, tool: "Bash", summary: "ls", questions: [] }) as unknown as Session["permission"];

describe("waitingRequests", () => {
  it("lists each wait with the hook's request when it has one", () => {
    const sessions = [
      session("a", "waitingPermission", { permission: permission("r1") }),
      session("b", "waitingInput", { phaseSince: 250 }),
      session("c", "generating"),
      session("d", "finished"),
    ];
    expect(waitingRequests(sessions, {})).toEqual([
      { session: "a", wait: "waitingPermission:100", id: "r1" },
      { session: "b", wait: "waitingInput:250", id: null },
    ]);
  });

  it("leaves out a Codex request it answers by itself", () => {
    const codex = session("x", "waitingPermission", { provider: "codex", permission: permission("r9") });
    expect(waitingRequests([codex], { x: true })).toEqual([]);
    expect(waitingRequests([codex], {})).toEqual([{ session: "x", wait: "waitingPermission:100", id: "r9" }]);
  });
});

describe("WaitingSound", () => {
  const request = (session: string, id: string | null, wait = "waitingPermission:5") => ({ session, wait, id });
  const a1 = request("a", "r1");
  const a2 = request("a", "r2");
  const b1 = request("b", null, "waitingInput:5");

  it("stays quiet for what was already waiting at start", () => {
    const sound = new WaitingSound();
    expect(sound.arrived([a1])).toEqual([]);
    expect(sound.arrived([a1])).toEqual([]);
  });

  it("reports each new request once", () => {
    const sound = new WaitingSound();
    sound.arrived([]);
    expect(sound.arrived([a1])).toEqual([a1]);
    expect(sound.arrived([a1])).toEqual([]);
    expect(sound.arrived([a1, b1])).toEqual([b1]);
    // The same session asking again is a new request.
    expect(sound.arrived([a2, b1])).toEqual([a2]);
    // Answered, then nothing waits: nothing to play.
    expect(sound.arrived([])).toEqual([]);
  });

  it("reports two requests that arrive together", () => {
    const sound = new WaitingSound();
    sound.arrived([]);
    expect(sound.arrived([a1, b1])).toEqual([a1, b1]);
  });

  it("plays once for a prompt the record tells of before the hook names it", () => {
    const sound = new WaitingSound();
    sound.arrived([]);
    const unnamed = request("a", null);
    expect(sound.arrived([unnamed])).toEqual([unnamed]);
    expect(sound.arrived([a1])).toEqual([]);
    expect(sound.arrived([unnamed])).toEqual([]);
    // The next prompt of the same wait still plays.
    expect(sound.arrived([a2])).toEqual([a2]);
  });
});

describe("inView", () => {
  it("is the open sessions page, on that session or on the list", () => {
    const open = { expanded: true, selected: "sessions" };
    expect(inView(open, null, "a")).toBe(true);
    expect(inView(open, "a", "a")).toBe(true);
    expect(inView(open, "b", "a")).toBe(false);
    expect(inView({ expanded: true, selected: "music" }, null, "a")).toBe(false);
    expect(inView({ expanded: false, selected: "sessions" }, null, "a")).toBe(false);
  });
});

describe("shouldPlay (#210)", () => {
  it("stays quiet when every unseen request's own terminal tab is in front", async () => {
    const asked: string[][] = [];
    const play = await shouldPlay(["a", "b"], async (ids) => {
      asked.push(ids);
      return true;
    });
    expect(play).toBe(false);
    expect(asked).toEqual([["a", "b"]]);
  });

  it("plays when a terminal shows something else or can't tell", async () => {
    expect(await shouldPlay(["a"], async () => false)).toBe(true);
    // Outside the app, the bridge answers nothing.
    expect(await shouldPlay(["a"], async () => undefined)).toBe(true);
  });

  it("plays when the terminals fail or are slow to answer", async () => {
    expect(await shouldPlay(["a"], () => Promise.reject(new Error("gone")))).toBe(true);
    expect(await shouldPlay(["a"], () => new Promise(() => {}), 10)).toBe(true);
  });

  it("asks nothing and plays nothing when the island shows every request (#167)", async () => {
    let asked = false;
    const play = await shouldPlay([], async () => {
      asked = true;
      return false;
    });
    expect(play).toBe(false);
    expect(asked).toBe(false);
  });
});
