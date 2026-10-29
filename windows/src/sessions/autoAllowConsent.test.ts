// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { autoAllowConsent } from "./autoAllowConsent";
import { demoSnapshot } from "./demo";
import type { Session } from "./types";

// As the app reports Codex sessions: no pid, Codex's own turn start.
const NOW = 1_800_000_000;
const session: Session = {
  ...demoSnapshot().sessions[0]!,
  provider: "codex",
  pid: 0,
  phase: "generating",
  turnStarted: NOW - 60,
};
const consent = (now = NOW) => autoAllowConsent(now);
beforeEach(() => {
  const data = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => data.set(key, value),
  });
});
afterEach(() => vi.unstubAllGlobals());

it("restores explicit Autoaccept for the same task after restart and initial empty discovery", () => {
  consent().save([session], { [session.id]: true });
  const restarted = consent();
  expect(restarted.restore([])).toEqual({});
  restarted.save([], {});
  expect(consent().restore([session])).toEqual({ [session.id]: true });
});

it("waits for the turn's exact start after a restart, without activating early", () => {
  consent().save([session], { [session.id]: true });
  const restarted = consent();
  // Read back before Codex reports the running turn: no start, or when the island saw it.
  for (const turnStarted of [undefined, NOW + 5]) {
    const early = { ...session, turnStarted };
    expect(restarted.restore([early])).toEqual({});
    restarted.save([early], {});
  }
  expect(restarted.restore([session])).toEqual({ [session.id]: true });
  restarted.save([session], { [session.id]: true });
  expect(consent().restore([session])).toEqual({ [session.id]: true });
});

it.each([{ turnStarted: NOW - 30 }, { cwd: "C:/another" }, { provider: "claude" as const }])(
  "never restores consent for another turn or thread: %j",
  (patch) => {
    consent().save([session], { [session.id]: true });
    expect(consent().restore([{ ...session, ...patch }])).toEqual({});
  },
);

it.each([{ phase: "finished" as const }, { phase: "idle" as const }])(
  "drops consent for a turn that ended while the app was closed: %j",
  (patch) => {
    consent().save([session], { [session.id]: true });
    const restarted = consent();
    expect(restarted.restore([{ ...session, ...patch }])).toEqual({});
    restarted.save([{ ...session, ...patch }], {});
    expect(consent().restore([session])).toEqual({});
  },
);

it("persists cancellation and never saves a turn without a known start", () => {
  const store = consent();
  store.save([session], { [session.id]: true });
  store.save([session], { [session.id]: false });
  expect(consent().restore([session])).toEqual({});
  store.save([{ ...session, turnStarted: undefined }], { [session.id]: true });
  expect(consent().restore([session])).toEqual({});
});

it("a new choice replaces a grant still waiting for its turn", () => {
  consent().save([session], { [session.id]: true });
  const restarted = consent();
  const next = { ...session, turnStarted: NOW + 10 };
  expect(restarted.restore([next])).toEqual({});
  restarted.save([next], { [session.id]: true });
  expect(consent().restore([next])).toEqual({ [session.id]: true });
});

it("forgets grants for threads that never come back after a day", () => {
  consent().save([session], { [session.id]: true });
  expect(consent(NOW + 23 * 3600).restore([session])).toEqual({ [session.id]: true });
  consent(NOW + 25 * 3600).save([], {});
  expect(localStorage.getItem("sessions.autoAllowTurns")).toBe("{}");
});

it("ignores malformed storage and continues without storage access", () => {
  localStorage.setItem(
    "sessions.autoAllowTurns",
    '{"invalid":{"cwd":7},"other":null,"x":{"cwd":"C:/","turnStarted":"1"}}',
  );
  expect(consent().restore([session])).toEqual({});
  vi.stubGlobal("localStorage", undefined);
  expect(() => consent().save([session], { [session.id]: true })).not.toThrow();
});
