// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
import { expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import { dotGeometry, iconSize } from "./trayIcon";
import { badgeText, menuTitle, NewAsks, trayAsks, trayModel } from "./trayState";
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
    title: undefined,
    name: id,
    project: id,
    phase: list.length ? "waitingPermission" : "idle",
    phaseSince: 100,
    permission: list[0],
    laterAsks: list.slice(1),
    ...extra,
  };
}

it("keeps a menu line to one readable line", () => {
  expect(menuTitle("first\nsecond")).toBe("first");
  const long = "x".repeat(120);
  expect([...menuTitle(long)].length).toBe(72);
  expect(menuTitle(long).endsWith("…")).toBe(true);
  expect(menuTitle("short")).toBe("short");
});

it("counts from two, the dot telling one", () => {
  expect([0, 1, 2, 9, 10, 31].map(badgeText)).toEqual(["", "", "2", "9", "9+", "9+"]);
  expect(dotGeometry(0, false)).toBeNull();
  const one = dotGeometry(1, false)!;
  const two = dotGeometry(2, false)!;
  expect(two.r).toBeGreaterThan(one.r);
  expect(dotGeometry(2, true)!.r).toBeGreaterThan(two.r);
  // The dot stays on the picture.
  expect(two.x + two.r).toBeLessThanOrEqual(32);
  expect(two.y - two.r).toBeGreaterThanOrEqual(0);
});

it("draws at Windows' small icon size for the screen's scale, and large for Linux panels", () => {
  expect(iconSize(false, 1)).toBe(16);
  expect(iconSize(false, 1.5)).toBe(24);
  expect(iconSize(false, 2)).toBe(32);
  expect(iconSize(false, 10)).toBe(64);
  expect(iconSize(false, Number.NaN)).toBe(16);
  expect(iconSize(true, 1)).toBe(48);
});

it("lists the asks as the queue does, with Allow and Deny only where it answers in place", () => {
  const sessions = [
    session("api", [ask("a1", 2, { detail: "rm -rf build\nnpm run build", risk: "deletes" }), ask("a2", 6)]),
    session("plan", [ask("p1", 1, { tool: "ExitPlanMode", detail: "# Plan" })]),
    session("ask", [ask("q1", 3, { questions: [{ question: "Which?", options: [], multiSelect: false }] })]),
    session("long", [
      ask("l1", 4, {
        command: {
          revision: "r",
          complete: false,
          truncated: true,
          remaining: 10,
          requiresInspection: true,
          available: true,
        },
      }),
    ]),
  ];
  const asks = trayAsks(sessions, { api: "Checkout" });
  expect(asks.map((a) => a.request)).toEqual(["p1", "a1", "q1", "l1", "a2"]);
  const byId = Object.fromEntries(asks.map((a) => [a.request, a]));
  expect(byId.a1!.decides && byId.a2!.decides).toBe(true);
  expect(byId.p1!.decides || byId.q1!.decides || byId.l1!.decides).toBe(false);
  expect(byId.a1!.title).toBe("Checkout — rm -rf build");
});

it("tells the icon what waits, in the person's words", () => {
  const snapshot = { ...demoSnapshot(), sessions: [session("api", [ask("a1", 2)])] };
  snapshot.counts = { ...snapshot.counts, waiting: 3 };
  const model = trayModel({ snapshot, autoAllow: {}, names: {} }, true);
  expect(model.visible).toBe(true);
  expect(model.waiting).toBe(3);
  expect(model.header).toBe("3 sessions are waiting for you");
  expect(model.tooltip).toBe("3 sessions are waiting for you");
  expect(model.label).toBe("3");
  expect(model.asks).toHaveLength(1);
  const none = trayModel(
    { snapshot: { ...snapshot, counts: { ...snapshot.counts, waiting: 0 } }, autoAllow: {}, names: {} },
    false,
  );
  expect(none.header).toBe("Nothing is waiting for you");
  expect(none.tooltip).toBe("Openbar Sessions");
  expect(none.label).toBe("");
  const one = trayModel(
    { snapshot: { ...snapshot, counts: { ...snapshot.counts, waiting: 1 } }, autoAllow: {}, names: {} },
    true,
  );
  expect(one.header).toBe("1 session is waiting for you");
});

it("pulses once for each new wait, never for the ones already there", () => {
  const asks = new NewAsks();
  const first = [session("api", [ask("a1", 2)])];
  expect(asks.arrived(first)).toBe(false);
  expect(asks.arrived(first)).toBe(false);
  expect(asks.arrived([session("api", [ask("a1", 2), ask("a2", 5)])])).toBe(true);
  expect(asks.arrived([session("api", [ask("a2", 5)])])).toBe(false);
  expect(
    asks.arrived([session("api", [ask("a2", 5)]), session("web", [], { phase: "waitingInput", phaseSince: 7 })]),
  ).toBe(true);
});
