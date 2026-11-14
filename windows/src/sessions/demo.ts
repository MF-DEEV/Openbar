// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample sessions for designing the section in a plain browser (?sessions).

import type { AwayLine } from "./away";
import type { Session, Snapshot } from "./types";

function base(over: Partial<Session> & Pick<Session, "id" | "project" | "phase">): Session {
  const t = Date.now() / 1000;
  return {
    provider: "claude",
    pid: 1000 + over.id.length,
    cwd: `C:\\Users\\vors\\Projekty\\${over.project}`,
    name: over.project,
    host: "windowsTerminal",
    acceptsReplies: true,
    phaseSince: t - 42,
    timeline: [],
    subagents: [],
    tokens: { input: 1200, cacheWrite: 30000, cacheRead: 120000, output: 4200 },
    cost: 0.19,
    hooked: true,
    chat: [],
    services: 0,
    ...over,
  };
}

export function demoSnapshot(): Snapshot {
  const t = Date.now() / 1000;
  const sessions: Session[] = [
    base({
      id: "a1",
      project: "checkout-api",
      phase: "waitingPermission",
      branch: "feat/apple-pay",
      title: "Npm test suite failures",
      permission: {
        requestId: "r1",
        tool: "Bash",
        summary: "npm test -- --runInBand",
        detail: "npm test -- --runInBand",
        offersAlways: true,
        since: t - 12,
        questions: [],
        fitsOnALine: true,
      },
      currentTool: { id: "t1", name: "Bash", summary: "npm test -- --runInBand", started: t - 12, failed: false },
    }),
    base({
      id: "b2",
      project: "landing-page",
      phase: "waitingInput",
      branch: "feat/new-hero",
      permission: {
        requestId: "r2",
        tool: "AskUserQuestion",
        summary: "Which headline should the hero use?",
        detail: "",
        offersAlways: false,
        since: t - 30,
        fitsOnALine: false,
        questions: [
          {
            question: "Which headline should the hero use?",
            header: "Headline",
            multiSelect: false,
            options: [
              { label: "Ship faster", description: "Short and direct, matches the old tone." },
              { label: "Your agents, at a glance", description: "Leads with the island." },
            ],
          },
        ],
      },
    }),
    base({
      id: "c3",
      project: "developer-docs",
      phase: "runningTool",
      branch: "docs/quickstart",
      turnStarted: t - 95,
      title: "Quickstart docs",
      currentTool: { id: "t2", name: "Edit", summary: "quickstart.md", started: t - 3, failed: false },
      timeline: [
        { id: "x1", name: "Read", summary: "README.md", started: t - 80, finished: t - 79, failed: false },
        { id: "x2", name: "Grep", summary: "install", started: t - 60, finished: t - 59, failed: false },
        { id: "x3", name: "Bash", summary: "npm run build", started: t - 40, finished: t - 20, failed: true },
        { id: "t2", name: "Edit", summary: "quickstart.md", started: t - 3, failed: false },
      ],
      subagents: [{ id: "s1", type: "Explore", description: "Find every install snippet", started: t - 70 }],
      model: "claude-opus-5-5",
      effort: "high",
      tasks: {
        done: 2,
        total: 5,
        current: "Rewrite the install section",
        items: [
          { title: "Read the current quickstart", state: "done" },
          { title: "List every install snippet", state: "done" },
          { title: "Rewrite the install section", state: "current" },
          { title: "Add the Windows steps", state: "pending" },
          { title: "Check the code blocks build", state: "pending" },
        ],
      },
      edits: { added: 42, removed: 7, touched: [t - 60, t - 30, t - 3] },
    }),
    base({
      id: "e5",
      provider: "codex",
      project: "payments-service",
      phase: "generating",
      branch: "fix/retry",
      turnStarted: t - 31,
      host: "unknown",
      title: "Retry failed webhooks",
      model: "gpt-5.5-codex",
      effort: "medium",
      tasks: { done: 1, total: 4 },
    }),
    base({
      id: "d5",
      provider: "codex",
      project: "data-pipeline",
      phase: "limited",
      branch: "main",
      host: "unknown",
      title: "Backfill the events table",
      phaseSince: t - 600,
      // The plan's limit stopped it; Codex names no reset, the limits reading has it.
      limitStop: { at: t - 600 },
    }),
    base({
      id: "d4",
      project: "ios-client",
      phase: "finished",
      branch: "feat/dark-mode",
      title: "SettingsView.swift overview",
      model: "claude-sonnet-5-5",
      effort: "medium",
      lastText: "Dark mode is in, every screen follows the system setting.",
      edits: { added: 128, removed: 34, touched: [t - 900, t - 700, t - 400, t - 260, t - 230] },
      phaseSince: t - 200,
      cost: 0.17,
    }),
  ];
  // ?sessions&accounts: Claude sessions under two accounts carry their names (#216).
  if (typeof location !== "undefined" && new URLSearchParams(location.search).has("accounts")) {
    for (const session of sessions) {
      if (session.provider !== "claude") continue;
      const work = session.id === "b2" || session.id === "d4";
      session.account = work ? "C:\\Users\\vors\\.claude-work" : "C:\\Users\\vors\\.claude";
      session.accountId = work ? "c:\\users\\vors\\.claude-work" : "c:\\users\\vors\\.claude";
      session.accountLabel = work ? "Work" : "Personal";
    }
  }
  // ?sessions&calm: only sessions at work and done, so the strip shows the agent's mark (#215).
  // ?sessions&calm=claude leaves Claude's alone at work.
  const calm = typeof location !== "undefined" ? new URLSearchParams(location.search).get("calm") : null;
  if (calm !== null) {
    const claudeOnly = calm === "claude";
    return {
      sessions: sessions.filter(
        (s) => !s.phase.startsWith("waiting") && s.phase !== "limited" && (!claudeOnly || s.provider === "claude"),
      ),
      counts: { waiting: 0, working: claudeOnly ? 1 : 2, workingCodex: claudeOnly ? 0 : 1, limited: 0, finished: 1 },
      previews: true,
      hooksInstalled: true,
    };
  }
  return {
    sessions,
    counts: { waiting: 2, working: 2, workingCodex: 1, limited: 1, finished: 1 },
    previews: true,
    hooksInstalled: true,
  };
}

/** The away card's tiles for the samples (?sessions&away). */
export function demoAwayLines(): AwayLine[] {
  return [
    { id: "a1", outcome: "waiting", files: 2, added: 18, removed: 3 },
    { id: "d5", outcome: "limited", files: 1, added: 9, removed: 0 },
    { id: "d4", outcome: "finished", files: 5, added: 128, removed: 34 },
    { id: "c3", outcome: "working", files: 3, added: 42, removed: 7, tasks: { from: 0, to: 2, total: 5 } },
    { id: "e5", outcome: "working", files: 0, added: 0, removed: 0, tasks: { from: 0, to: 1, total: 4 } },
  ];
}
