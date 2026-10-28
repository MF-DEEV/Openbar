// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/// <reference types="node" />

import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
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
