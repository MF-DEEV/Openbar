// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { SessionsList } from "./SessionsList";
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

it("dims the whole row of a sleeping session, not its avatar alone", () => {
  const html = rows([quiet]);
  expect(html).toContain('class="s-row is-dozing"');
  expect(html).not.toContain("opacity:0.65");
});

it("keeps a row whole while its session works, was just looked at, or has no Clawd", () => {
  expect(rows([{ ...quiet, phase: "finished" }])).not.toContain("is-dozing");
  expect(rows([{ ...quiet, lastActive: now - 60 }])).not.toContain("is-dozing");
  expect(rows([quiet], false)).not.toContain("is-dozing");
  useSessions.setState({ lookedAt: { quiet: now - 10 } });
  expect(renderToStaticMarkup(<SessionsList />)).not.toContain("is-dozing");
});
