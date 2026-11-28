// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/// <reference types="node" />

import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SessionsList } from "./SessionsList";
import { DOZING_OPACITY } from "./clawd/sprite";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";
import type { Session } from "./types";

// Server rendering asks Zustand for its initial snapshot; read the test state.
vi.mock("./store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./store")>();
  const store = actual.useSessions;
  return {
    ...actual,
    useSessions: Object.assign(
      (select: (state: ReturnType<typeof store.getState>) => unknown) => select(store.getState()),
      store,
    ),
  };
});

const now = Date.now() / 1000;
const quiet: Session = {
  ...demoSnapshot().sessions[0]!,
  id: "quiet",
  phase: "idle",
  permission: undefined,
  phaseSince: now - 900,
  lastActive: now - 900,
};

function rows(sessions: Session[], buddies = true): string {
  useSessions.setState((s) => ({
    snapshot: { ...demoSnapshot(), sessions },
    prefs: { ...s.prefs, buddies },
    lookedAt: {},
    autoAllow: {},
  }));
  return renderToStaticMarkup(<SessionsList />);
}

beforeEach(() => useSessions.setState({ lookedAt: {} }));
afterEach(() => vi.unstubAllGlobals());

it("dims the whole row of a sleeping session, not its avatar alone", () => {
  const html = rows([quiet]);
  expect(html).toContain('class="s-row is-dozing"');
  expect(html).not.toContain("opacity:0.65");
});

it("keeps a row whole while its session works, was just looked at, or has no Clawd", () => {
  expect(rows([{ ...quiet, phase: "finished" }])).not.toContain("is-dozing");
  expect(rows([{ ...quiet, lastActive: now - 60 }])).not.toContain("is-dozing");
  expect(rows([quiet], false)).not.toContain("is-dozing");
  expect(rows([{ ...quiet, lookedAt: now - 10 }])).not.toContain("is-dozing");
  useSessions.setState({ lookedAt: { quiet: now - 10 } });
  expect(renderToStaticMarkup(<SessionsList />)).not.toContain("is-dozing");
});

it("starts fading at five idle minutes, and fades nothing with Clawds off", () => {
  expect(rows([{ ...quiet, lastActive: now - 300 }])).toContain("is-dozing");
  expect(rows([{ ...quiet, lastActive: now - 290 }])).not.toContain("is-dozing");
  expect(rows([quiet], false)).not.toContain("opacity:0.65");
});

it("fades a sleeping row to DOZING_OPACITY in the stylesheet", () => {
  const css = readFileSync(new URL("./sessions.css", import.meta.url), "utf8");
  expect(css).toMatch(
    new RegExp(`\\.s-row\\.is-dozing:not\\(:hover, :focus-visible\\)[^{]*\\{\\s*opacity: ${DOZING_OPACITY};`),
  );
});

it("pins each row's Clawd with its agent's mark and names the agent", () => {
  const html = rows([
    { ...quiet, id: "c", provider: "claude", title: "Crab" },
    { ...quiet, id: "x", provider: "codex", title: "Robot" },
  ]);
  expect(html).toContain('class="s-agent-mark" data-provider="claude"');
  expect(html).toContain('class="s-agent-mark" data-provider="codex"');
  expect(html).toMatch(/aria-label="Crab, Claude Code, /);
  expect(html).toMatch(/aria-label="Robot, Codex, /);
});

it("leaves the mark off without Clawds, where the badge already shows the agent at work", () => {
  expect(rows([{ ...quiet, provider: "codex" }], false)).not.toContain("s-agent-mark");
});

it("names the SSH host of a remote session beside its name, and only then", () => {
  const remote = rows([
    { ...quiet, id: "far", host: "remote", remote: "openbar-linux", hostName: "tmux on openbar-linux" },
  ]);
  expect(remote).toContain('class="s-remote-mark"');
  expect(remote).toContain("openbar-linux");
  expect(rows([quiet])).not.toContain("s-remote-mark");
  const long = rows([{ ...quiet, id: "far", host: "remote", remote: "a-very-long-host-alias.example.com" }]);
  expect(long).toContain("a-very-long-host-alia…");
});

it("names the tmux or zellij a local session's pane belongs to (#235)", () => {
  const zellij = rows([{ ...quiet, id: "z", host: "zellij", hostName: "zellij" }]);
  expect(zellij).toContain('class="s-multiplexer-mark"');
  expect(zellij).toContain("In a zellij pane");
  expect(rows([{ ...quiet, id: "t", host: "tmux", hostName: "tmux" }])).toContain(">tmux</span>");
  // #328: Herdr's pane, named as Herdr names itself.
  const herdr = rows([{ ...quiet, id: "h", host: "herdr", hostName: "Herdr" }]);
  expect(herdr).toContain("In a Herdr pane");
  expect(herdr).toContain(">Herdr</span>");
  expect(rows([quiet])).not.toContain("s-multiplexer-mark");
  // A remote host's row names the host instead.
  expect(rows([{ ...quiet, id: "far", host: "remote", remote: "box" }])).not.toContain("s-multiplexer-mark");
});

it("names the terminals and editors of the platform in the empty list", () => {
  expect(rows([])).toContain("Windows Terminal or VS Code");
  vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)" });
  const html = rows([]);
  expect(html).toContain("in a terminal or an editor");
  expect(html).not.toContain("Windows Terminal");
});

function grouped(sessions: Session[], grouping: "none" | "project" | "agent", collapsedGroups: string[] = []): string {
  useSessions.setState((s) => ({
    snapshot: { ...demoSnapshot(), sessions },
    prefs: { ...s.prefs, grouping, collapsedGroups },
    lookedAt: {},
    autoAllow: {},
  }));
  const html = renderToStaticMarkup(<SessionsList />);
  useSessions.setState((s) => ({ prefs: { ...s.prefs, grouping: "none", collapsedGroups: [] } }));
  return html;
}

it("keeps today's single list under None (#233)", () => {
  const html = grouped(demoSnapshot().sessions, "none");
  expect(html).not.toContain("s-group");
  expect(html.match(/class="s-row[ "]/g)).toHaveLength(demoSnapshot().sessions.length);
});

it("heads each project's group and drops the repeated project tag from its rows (#233)", () => {
  const sessions = demoSnapshot().sessions;
  const html = grouped(sessions, "project");
  expect(html.match(/class="s-group-head/g)).toHaveLength(new Set(sessions.map((s) => s.project)).size);
  expect(html).not.toContain('class="s-tag"');
  expect(html).toContain('aria-expanded="true"');
});

it("folds a group to its header, which still says someone waits (#233)", () => {
  const sessions = demoSnapshot().sessions;
  const waiting = sessions.find((s) => s.phase === "waitingPermission")!;
  const html = grouped(sessions, "project", [`project::${waiting.project}`]);
  expect(html).toContain('class="s-group-head is-collapsed has-waiting"');
  expect(html).toContain('aria-expanded="false"');
  expect(html).toContain("s-group-faces");
  // The queue on top still lists its ask (#269); the folded group does not.
  const groups = html.slice(html.indexOf('class="s-group '));
  expect(groups).not.toContain(waiting.title!);
});

it("puts every waiting ask in one queue above the rows, without row buttons (#269)", () => {
  const sessions = demoSnapshot().sessions;
  const html = grouped(sessions, "project");
  expect(html.indexOf('class="s-queue"')).toBeGreaterThan(-1);
  expect(html.indexOf('class="s-queue"')).toBeLessThan(html.indexOf('<section class="s-group'));
  // The rows below only say they wait; the queue answers them.
  expect(html.slice(html.indexOf('<section class="s-group')).includes('class="s-quick"')).toBe(false);
});

it("groups by agent and keeps the project tag on rows (#233)", () => {
  const html = grouped(demoSnapshot().sessions, "agent");
  expect(html).toContain(">Claude Code<");
  expect(html).toContain(">Codex<");
  expect(html).toContain('class="s-tag"');
});

it("shows the branch beside the project, cut in the middle, whole on hover (#233)", () => {
  const long = "fix/currency-rounding-for-the-new-checkout-totals";
  const html = rows([{ ...quiet, branch: long }]);
  expect(html).toContain(`title="${long}"`);
  expect(html).toContain("…");
  expect(html.indexOf("s-tag")).toBeLessThan(html.indexOf("s-branch-chip"));
  expect(rows([{ ...quiet, branch: undefined }])).not.toContain("s-branch-chip");
});
