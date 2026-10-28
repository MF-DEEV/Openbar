// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { demoSnapshot } from "./demo";
import { subscribeAutoAllow } from "./autoAllow";
import { useSessions } from "./store";
import type { Session } from "./types";

const sample = demoSnapshot().sessions.find((s) => !!s.permission && !s.permission.questions.length)!;
const codex: Session = { ...sample, provider: "codex", turnStarted: 100 };
let stop: () => void;
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(bridge.sessions, "decide").mockResolvedValue(true);
  useSessions.setState({ autoAllow: {}, snapshot: { ...demoSnapshot(), sessions: [] }, shownAt: {} });
  stop = subscribeAutoAllow();
});
afterEach(() => {
  stop();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
function receive(sessions: Session[]) {
  useSessions.getState().receive({ ...demoSnapshot(), sessions });
}

it("one click approves the current and subsequent requests with the page closed, once each", async () => {
  receive([codex]);
  useSessions.setState({ focused: null });
  useSessions.getState().toggleAutoAllow(codex.id);
  await vi.advanceTimersByTimeAsync(500);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(400);
  expect(bridge.sessions.decide).toHaveBeenCalledExactlyOnceWith(codex.id, codex.permission!.requestId, "allow");
  receive([codex]);
  await vi.advanceTimersByTimeAsync(2000);
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(1);
  receive([{ ...codex, permission: { ...codex.permission!, requestId: "next" } }]);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).toHaveBeenLastCalledWith(codex.id, "next", "allow");
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(2);
});

it("does not enable itself or approve other sessions, Claude, or question forms", async () => {
  const question = {
    ...codex,
    id: "question",
    permission: {
      ...codex.permission!,
      requestId: "q",
      questions: [{ question: "Which?", options: [], multiSelect: false }],
    },
  };
  const claude: Session = { ...codex, id: "claude", provider: "claude" };
  receive([codex, question, claude]);
  useSessions.getState().toggleAutoAllow(claude.id);
  expect(useSessions.getState().autoAllow[claude.id]).toBeUndefined();
  useSessions.getState().toggleAutoAllow(question.id);
  await vi.advanceTimersByTimeAsync(2000);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
});

it("turning off cancels pending approvals and ignores requests resolved elsewhere", async () => {
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  useSessions.getState().toggleAutoAllow(codex.id);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
  useSessions.getState().toggleAutoAllow(codex.id);
  receive([{ ...codex, phase: "runningTool", permission: undefined }]);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
});

it("failed approvals turn auto allow off instead of resending continuously", async () => {
  vi.mocked(bridge.sessions.decide).mockRejectedValue(new Error("Disconnected"));
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  await vi.advanceTimersByTimeAsync(900);
  expect(useSessions.getState().autoAllow[codex.id]).toBe(false);
  receive([codex]);
  await vi.advanceTimersByTimeAsync(2000);
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(1);
});

it.each(["finished", "idle"] as const)("%s resets Always and the next prompt needs a new choice", async (phase) => {
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(1);
  receive([{ ...codex, phase, permission: undefined }]);
  expect(useSessions.getState().autoAllow[codex.id]).not.toBe(true);
  useSessions.getState().toggleAutoAllow(codex.id);
  expect(useSessions.getState().autoAllow[codex.id]).not.toBe(true);
  const next = { ...codex, turnStarted: 200, permission: { ...codex.permission!, requestId: "new-prompt" } };
  receive([next]);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).toHaveBeenCalledTimes(1);
  useSessions.getState().toggleAutoAllow(codex.id);
  await vi.advanceTimersByTimeAsync(50);
  expect(bridge.sessions.decide).toHaveBeenLastCalledWith(codex.id, "new-prompt", "allow");
});

it("a new turn resets Always even when no finished snapshot was received", async () => {
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  receive([{ ...codex, turnStarted: 200, permission: { ...codex.permission!, requestId: "new-turn" } }]);
  await vi.advanceTimersByTimeAsync(900);
  expect(useSessions.getState().autoAllow[codex.id]).not.toBe(true);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
});

it("finishing cancels queued approvals and removing the session clears consent", async () => {
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  receive([{ ...codex, phase: "finished", permission: undefined }]);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  receive([]);
  receive([codex]);
  await vi.advanceTimersByTimeAsync(900);
  expect(useSessions.getState().autoAllow[codex.id]).not.toBe(true);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
});

it("tool changes and questions within the same turn preserve Always", async () => {
  receive([codex]);
  useSessions.getState().toggleAutoAllow(codex.id);
  receive([{ ...codex, phase: "runningTool", permission: undefined }]);
  receive([
    {
      ...codex,
      phase: "waitingInput",
      permission: {
        ...codex.permission!,
        requestId: "question",
        questions: [{ question: "Which?", options: [], multiSelect: false }],
      },
    },
  ]);
  await vi.advanceTimersByTimeAsync(900);
  expect(useSessions.getState().autoAllow[codex.id]).toBe(true);
  expect(bridge.sessions.decide).not.toHaveBeenCalled();
  receive([{ ...codex, permission: { ...codex.permission!, requestId: "after-question" } }]);
  await vi.advanceTimersByTimeAsync(900);
  expect(bridge.sessions.decide).toHaveBeenCalledExactlyOnceWith(codex.id, "after-question", "allow");
});
