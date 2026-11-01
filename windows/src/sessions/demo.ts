// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample sessions for designing the section in a plain browser (?sessions).

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
      tasks: { done: 2, total: 5, current: "Rewrite the install section" },
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
      phaseSince: t - 200,
      cost: 0.17,
    }),
  ];
  return {
    sessions,
    counts: { waiting: 2, working: 2, workingCodex: 1, limited: 1, finished: 1 },
    previews: true,
    hooksInstalled: true,
  };
}
