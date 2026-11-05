// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A plan's card offers the modes Claude Code's plan dialog offers, and the
// store sends the picked one along with the allow.

import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { PermissionCard, PLAN_TOOL } from "./PermissionCard";
import { demoSnapshot } from "./demo";
import { SETTLE_DELAY, useSessions } from "./store";
import { bridge } from "../shared/bridge";
import type { PlanMode, Session } from "./types";

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

beforeEach(() => {
  useSessions.setState({ snapshot: demoSnapshot(), autoAllow: {} });
});

function plan(planModes?: PlanMode[]): Session {
  const base = demoSnapshot().sessions[0]!;
  return {
    ...base,
    permission: {
      ...base.permission!,
      tool: PLAN_TOOL,
      detail: "# Plan\n\n1. Do it",
      offersAlways: false,
      diffs: [],
      planModes,
      fitsOnALine: false,
    },
  };
}

it("puts the mode that lets Claude work on its own first, and manual beside it", () => {
  const html = renderToStaticMarkup(<PermissionCard session={plan(["auto", "default"])} onOpen={() => {}} />);
  expect(html).toContain("Keep planning");
  expect(html).toContain("Manual");
  expect(html).toContain("Auto mode");
  expect(html).not.toContain(">Approve<");
  // Manual sits before the green button, which Ctrl+Y presses.
  expect(html.indexOf("Manual")).toBeLessThan(html.indexOf("Auto mode"));
  expect(html).toMatch(/class="s-button is-allow"[^>]*aria-keyshortcuts="Control\+y"[^>]*>.*Auto mode/s);
  expect(html).toContain("Ctrl+Y approves with Auto mode");
});

it("offers accept edits where auto mode is not known to work", () => {
  const html = renderToStaticMarkup(<PermissionCard session={plan(["acceptEdits", "default"])} onOpen={() => {}} />);
  expect(html).toContain("Accept edits");
  expect(html).not.toContain("Auto mode");
  expect(html).toContain("Ctrl+Y approves with Accept edits");
});

it("keeps a plain Approve when the service sent no modes", () => {
  const html = renderToStaticMarkup(<PermissionCard session={plan(undefined)} onOpen={() => {}} />);
  expect(html).toContain("Approve");
  expect(html).not.toContain("Manual");
});

it("sends the picked mode with the allow, and nothing extra otherwise", async () => {
  const session = plan(["auto", "default"]);
  const shownAt = Date.now() / 1000 - SETTLE_DELAY - 1;
  useSessions.setState({ shownAt: { [session.permission!.requestId]: shownAt } });
  const decide = vi.spyOn(bridge.sessions, "decide").mockResolvedValue(true);
  await useSessions.getState().decide(session, "allow", "default");
  expect(decide).toHaveBeenLastCalledWith(session.id, session.permission!.requestId, "allow", "default");
  await useSessions.getState().decide(session, "deny");
  expect(decide).toHaveBeenLastCalledWith(session.id, session.permission!.requestId, "deny");
  decide.mockRestore();
});
