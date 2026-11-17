// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import { CODEX_TINT, phaseTint } from "./format";
import { presentedSnapshot } from "./presentation";
import type { Session, Snapshot } from "./types";

const codex: Session = { ...demoSnapshot().sessions[0]!, provider: "codex", turnStarted: 100 };
const snapshot = (sessions: Session[]): Snapshot => ({
  ...demoSnapshot(),
  sessions,
  counts: { working: 0, workingCodex: 0, waiting: sessions.length, finished: 0 },
});

it("presents automatic approvals as blue working immediately while preserving the real request", () => {
  const raw = snapshot([codex]);
  const state = { snapshot: raw, autoAllow: { [codex.id]: true } };
  const shown = presentedSnapshot(state);
  expect(shown.sessions[0]).toMatchObject({ phase: "generating", phaseSince: 100, permission: undefined });
  expect(phaseTint(shown.sessions[0]!.phase, "codex")).toBe(CODEX_TINT);
  expect(shown.counts).toEqual({ working: 1, workingCodex: 1, waiting: 0, finished: 0 });
  expect(raw.sessions[0]!.permission).toBe(codex.permission);
  expect(raw.sessions[0]!.phase).toBe("waitingPermission");
  expect(presentedSnapshot(state)).toBe(shown);
});

it("also hides status-only frames around the request so the hand cannot flash", () => {
  for (const permission of [undefined, codex.permission, undefined]) {
    const shown = presentedSnapshot({
      snapshot: snapshot([{ ...codex, permission }]),
      autoAllow: { [codex.id]: true },
    });
    expect(shown.sessions[0]!.phase).toBe("generating");
    expect(shown.counts.waiting).toBe(0);
  }
});

it("keeps real questions, terminal-only prompts, Claude and other sessions visible", () => {
  const sessions: Session[] = [
    codex,
    { ...codex, id: "other" },
    { ...codex, id: "claude", provider: "claude" },
    {
      ...codex,
      id: "question",
      phase: "waitingInput",
      permission: { ...codex.permission!, questions: [{ question: "Which?", options: [], multiSelect: false }] },
    },
    { ...codex, id: "terminal", waitingFor: "terminal" },
  ];
  const autoAllow = Object.fromEntries(sessions.filter((s) => s.id !== "other").map((s) => [s.id, true]));
  const raw = snapshot(sessions);
  const shown = presentedSnapshot({ snapshot: raw, autoAllow });
  expect(shown.counts).toEqual({ working: 1, workingCodex: 1, waiting: 4, finished: 0 });
  expect(shown.sessions.slice(1)).toEqual(raw.sessions.slice(1));
  for (let i = 1; i < sessions.length; i++) expect(shown.sessions[i]).toBe(sessions[i]);
});

it("turning Always off immediately restores the manual approval", () => {
  const raw = snapshot([codex]);
  expect(presentedSnapshot({ snapshot: raw, autoAllow: { [codex.id]: true } }).counts.waiting).toBe(0);
  expect(presentedSnapshot({ snapshot: raw, autoAllow: { [codex.id]: false } })).toBe(raw);
});
