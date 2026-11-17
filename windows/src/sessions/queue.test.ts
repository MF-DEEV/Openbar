// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { demoSnapshot } from "./demo";
import { asks, permissionQueue, showing } from "./queue";
import { useSessions } from "./store";
import type { Permission, PermissionRisk, Session } from "./types";

const base = demoSnapshot().sessions.find((s) => !!s.permission && !s.permission.questions.length)!;

function ask(requestId: string, since: number, extra: Partial<Permission> = {}): Permission {
  return {
    requestId,
    tool: "Bash",
    detail: "ls",
    offersAlways: false,
    since,
    questions: [],
    fitsOnALine: true,
    risk: "safe" as PermissionRisk,
    command: {
      revision: `rev-${requestId}`,
      complete: true,
      truncated: false,
      remaining: null,
      requiresInspection: false,
      available: true,
    },
    ...extra,
  };
}

function session(id: string, list: Permission[], extra: Partial<Session> = {}): Session {
  return {
    ...base,
    id,
    phase: list.length ? "waitingPermission" : "idle",
    permission: list[0],
    laterAsks: list.slice(1),
    ...extra,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(bridge.sessions, "decide").mockResolvedValue(true);
  useSessions.setState({ autoAllow: {}, snapshot: { ...demoSnapshot(), sessions: [] }, shownAt: {}, focused: null });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("lists every ask oldest first and keeps one ask on its own page", () => {
  expect(permissionQueue([session("a", [ask("a1", 1)]), session("b", [])]).shows).toBe(false);
  const queue = permissionQueue([
    session("a", [ask("a1", 5), ask("a2", 1, { detail: "rm -rf x", risk: "deletes", offersAlways: true })]),
    session("b", [ask("b1", 2, { tool: "ExitPlanMode", detail: "# Plan" })]),
    session("c", [ask("c1", 3, { questions: [{ question: "Which?", options: [], multiSelect: false }] })]),
    session("d", [ask("d1", 4, { command: { ...ask("x", 0).command!, requiresInspection: true } })]),
    session("e", [ask("e1", 6, { risk: "unassessed" })]),
    session("f", [ask("f1", 7)], { provider: "codex" }),
  ]);
  expect(queue.shows).toBe(true);
  expect(queue.rows.map((r) => r.ask.requestId)).toEqual(["a2", "b1", "c1", "d1", "a1", "e1", "f1"]);
  expect(queue.allowAll.map((r) => r.ask.requestId)).toEqual(["a1", "f1"]);
  expect(queue.heldBack).toBe(5);
  expect(queue.keyTarget?.ask.requestId).toBe("a2");
});

it("a new ask joins at the foot while the sessions reorder", () => {
  const before = permissionQueue([session("a", [ask("a1", 1)]), session("b", [ask("b1", 2)])]);
  const after = permissionQueue([session("b", [ask("b1", 2), ask("b2", 9)]), session("a", [ask("a1", 1)])]);
  expect(after.rows.slice(0, 2).map((r) => r.id)).toEqual(before.rows.map((r) => r.id));
  expect(after.rows.at(-1)?.ask.requestId).toBe("b2");
});

it("opens a page on an ask behind another", () => {
  const s = session("a", [ask("a1", 1), ask("a2", 2), ask("a3", 3)]);
  const shown = showing(s, "a3");
  expect(shown.permission?.requestId).toBe("a3");
  expect(asks(shown).map((p) => p.requestId)).toEqual(["a3", "a1", "a2"]);
  expect(showing(s, "gone")).toBe(s);
  expect(showing(s, null)).toBe(s);
});

it("Allow all answers only the safe asks that settled, each by its own id", async () => {
  const sessions = [session("a", [ask("a1", 1), ask("a2", 2, { risk: "deletes" })]), session("b", [ask("b1", 3)])];
  useSessions.getState().receive({ ...demoSnapshot(), sessions });
  const queue = permissionQueue(sessions);
  expect(await useSessions.getState().allowAll(queue)).toBe(0);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(900);
  expect(await useSessions.getState().allowAll(queue)).toBe(2);
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(2);
  expect(bridge.sessions.decide).toHaveBeenCalledWith("a", "a1", "allow", undefined, "rev-a1", undefined);
  expect(bridge.sessions.decide).toHaveBeenCalledWith("b", "b1", "allow", undefined, "rev-b1", undefined);
});

it("a later ask is answered on its own, and an ask no longer held is not", async () => {
  const s = session("a", [ask("a1", 1), ask("a2", 2)]);
  useSessions.getState().receive({ ...demoSnapshot(), sessions: [s] });
  await vi.advanceTimersByTimeAsync(900);
  expect(await useSessions.getState().decide(showing(s, "a2"), "deny")).toBe(true);
  expect(bridge.sessions.decide).toHaveBeenLastCalledWith("a", "a2", "deny", undefined, "rev-a2", undefined);
  useSessions.getState().receive({ ...demoSnapshot(), sessions: [session("a", [ask("a1", 1)])] });
  expect(await useSessions.getState().decide(showing(s, "a2"), "allow")).toBe(false);
});
