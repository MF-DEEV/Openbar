// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SessionNoticeMark } from "./SessionAvatar";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";
import { FINISHED_TINT, WAITING_TINT } from "./format";

// Server rendering asks Zustand for its initial snapshot; read the test state instead.
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

// What each Clawd was asked to draw, still drawn by the real one.
const drawn = vi.hoisted(() => [] as Record<string, unknown>[]);
vi.mock("./clawd/Clawd", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./clawd/Clawd")>();
  return {
    ...actual,
    Clawd: (props: Parameters<typeof actual.Clawd>[0]) => {
      drawn.push(props as unknown as Record<string, unknown>);
      return actual.Clawd(props);
    },
  };
});

const initial = useSessions.getState();
beforeEach(() => {
  vi.stubGlobal("window", { matchMedia: () => ({ matches: true }), devicePixelRatio: 2 });
  useSessions.setState({ snapshot: demoSnapshot() });
  drawn.length = 0;
});
afterEach(() => {
  vi.unstubAllGlobals();
  useSessions.setState(initial, true);
});

const id = () => demoSnapshot().sessions[0]!.id;

it("shows the session's Clawd, no taller than the strip allows", () => {
  const html = renderToStaticMarkup(
    <SessionNoticeMark session={{ id: id(), provider: "claude", event: "needsPermission" }} height={14} />,
  );
  expect(html).toContain("<canvas");
  expect(html).toContain("width:18px;height:14px");
});

it("still shows a Clawd for a session that has left the list", () => {
  const html = renderToStaticMarkup(
    <SessionNoticeMark session={{ id: "gone", provider: "codex", event: "finished" }} height={18} />,
  );
  expect(html).toContain("<canvas");
});

it("with buddies off shows the notice's symbol in its phase's tint", () => {
  useSessions.setState({ prefs: { ...useSessions.getState().prefs, buddies: false } });
  const waiting = renderToStaticMarkup(
    <SessionNoticeMark session={{ id: id(), provider: "claude", event: "needsInput" }} height={18} />,
  );
  expect(waiting).not.toContain("<canvas");
  expect(waiting).toContain(`color:${WAITING_TINT}`);
  // The slot keeps the Clawd's width, so the notice measures the same either way.
  expect(waiting).toContain("width:18px");
  const done = renderToStaticMarkup(
    <SessionNoticeMark session={{ id: id(), provider: "claude", event: "finished" }} height={18} />,
  );
  expect(done).toContain(`color:${FINISHED_TINT}`);
});

it("moves as the notice says and cheers only a finished turn", () => {
  const sessions = demoSnapshot().sessions;
  const finished = sessions.find((s) => s.phase === "finished")!;
  const waiting = sessions.find((s) => s.phase === "waitingPermission")!;
  for (const [id, event] of [
    [waiting.id, "needsPermission"],
    [waiting.id, "needsInput"],
    [finished.id, "finished"],
  ] as const)
    renderToStaticMarkup(<SessionNoticeMark session={{ id, provider: "claude", event }} height={18} />);
  expect(drawn.map((p) => p.motion)).toEqual(["alert", "ask", "content"]);
  expect(drawn.map((p) => p.bubbles)).toEqual([false, false, false]);
  expect(drawn[0]!.cheer).toBeNull();
  expect(drawn[2]!.cheer).toBe(finished.phaseSince * 1000);
  expect(drawn[2]!.since).toBe(finished.phaseSince * 1000);
});

it("draws the live session's agent, or the notice's once it has left the list", () => {
  const live = demoSnapshot().sessions.find((s) => s.provider === "codex")!;
  renderToStaticMarkup(
    <SessionNoticeMark session={{ id: live.id, provider: "claude", event: "finished" }} height={18} />,
  );
  renderToStaticMarkup(
    <SessionNoticeMark session={{ id: "gone", provider: "codex", event: "finished" }} height={18} />,
  );
  expect(drawn.map((p) => p.character)).toEqual(["codex", "codex"]);
  expect(drawn[1]!.since).toBeNull();
});
