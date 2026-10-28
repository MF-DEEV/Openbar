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

const initial = useSessions.getState();
beforeEach(() => {
  vi.stubGlobal("window", { matchMedia: () => ({ matches: true }), devicePixelRatio: 2 });
  useSessions.setState({ snapshot: demoSnapshot() });
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
  const done = renderToStaticMarkup(
    <SessionNoticeMark session={{ id: id(), provider: "claude", event: "finished" }} height={18} />,
  );
  expect(done).toContain(`color:${FINISHED_TINT}`);
});
