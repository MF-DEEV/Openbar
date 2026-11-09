// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { describe, expect, it } from "vitest";
import type { Session } from "./types";
import { inView, waitingRequests, WaitingSound } from "./waitingSound";

function session(id: string, phase: Session["phase"], extra: Partial<Session> = {}): Session {
  return { id, phase, phaseSince: 100, provider: "claude", ...extra } as Session;
}

const permission = (requestId: string) =>
  ({ requestId, tool: "Bash", summary: "ls", questions: [] }) as unknown as Session["permission"];

describe("waitingRequests", () => {
  it("lists permissions by request and questions by when they started", () => {
    const sessions = [
      session("a", "waitingPermission", { permission: permission("r1") }),
      session("b", "waitingInput", { phaseSince: 250 }),
      session("c", "generating"),
      session("d", "finished"),
    ];
    expect(waitingRequests(sessions, {})).toEqual([
      { session: "a", request: "r1" },
      { session: "b", request: "b:waitingInput:250" },
    ]);
  });

  it("leaves out a Codex request it answers by itself", () => {
    const codex = session("x", "waitingPermission", { provider: "codex", permission: permission("r9") });
    expect(waitingRequests([codex], { x: true })).toEqual([]);
    expect(waitingRequests([codex], {})).toEqual([{ session: "x", request: "r9" }]);
  });
});

describe("WaitingSound", () => {
  const a1 = { session: "a", request: "r1" };
  const a2 = { session: "a", request: "r2" };
  const b1 = { session: "b", request: "b:waitingInput:5" };

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
