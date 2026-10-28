// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { autoAllowConsent } from "./autoAllowConsent";
import { demoSnapshot } from "./demo";
import type { Session } from "./types";

const session: Session = { ...demoSnapshot().sessions[0]!, provider: "codex", turnStarted: 123 };
beforeEach(() => {
  const data = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => data.set(key, value),
  });
});
afterEach(() => vi.unstubAllGlobals());

it("restores explicit Autoaccept for the same task after restart and initial empty discovery", () => {
  autoAllowConsent().save([session], { [session.id]: true });
  const restarted = autoAllowConsent();
  expect(restarted.restore([])).toEqual({});
  restarted.save([], {});
  expect(autoAllowConsent().restore([session])).toEqual({ [session.id]: true });
});

it.each([
  { turnStarted: 456 },
  { pid: session.pid + 1 },
  { cwd: "C:/another" },
  { phase: "finished" as const },
  { phase: "idle" as const },
  { provider: "claude" as const },
])("does not restore consent for a different task or identity: %j", (patch) => {
  autoAllowConsent().save([session], { [session.id]: true });
  const restarted = autoAllowConsent();
  const changed = { ...session, ...patch };
  expect(restarted.restore([changed])).toEqual({});
  restarted.save([changed], {});
  expect(autoAllowConsent().restore([session])).toEqual({});
});

it("persists cancellation and never restores unknown turn identities", () => {
  const store = autoAllowConsent();
  store.save([session], { [session.id]: true });
  store.save([session], { [session.id]: false });
  expect(autoAllowConsent().restore([session])).toEqual({});
  store.save([{ ...session, turnStarted: undefined }], { [session.id]: true });
  expect(autoAllowConsent().restore([session])).toEqual({});
});

it("ignores malformed storage and continues without storage access", () => {
  localStorage.setItem("sessions.autoAllowTurns", '{"invalid":{"pid":"bad"}}');
  expect(autoAllowConsent().restore([session])).toEqual({});
  vi.stubGlobal("localStorage", undefined);
  expect(() => autoAllowConsent().save([session], { [session.id]: true })).not.toThrow();
});
