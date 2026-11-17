// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";

const session = { ...demoSnapshot().sessions[0]!, id: "z", host: "zellij" as const };

beforeEach(() => {
  useSessions.setState({ detached: null, openFailedUntil: 0 });
  vi.spyOn(bridge.sessions, "acknowledge").mockResolvedValue(undefined as never);
});

afterEach(() => vi.restoreAllMocks());

it("offers Attach when Open finds the session's zellij detached (#235)", async () => {
  vi.spyOn(bridge.sessions, "open").mockResolvedValue("detached");
  expect(await useSessions.getState().openWindow(session)).toBe(false);
  expect(useSessions.getState().detached?.id).toBe("z");
  expect(useSessions.getState().openFailedUntil).toBe(0);

  const attach = vi.spyOn(bridge.sessions, "attach").mockResolvedValue(true);
  expect(await useSessions.getState().attachDetached(session)).toBe(true);
  expect(attach).toHaveBeenCalledExactlyOnceWith("z");
  expect(useSessions.getState().detached).toBeNull();
});

it("says Open failed when nothing is found, and clears an old Attach offer", async () => {
  useSessions.setState({ detached: { id: "z", until: Date.now() / 1000 + 10 } });
  vi.spyOn(bridge.sessions, "open").mockResolvedValue("missing");
  expect(await useSessions.getState().openWindow(session)).toBe(false);
  expect(useSessions.getState().detached).toBeNull();
  expect(useSessions.getState().openFailedUntil).toBeGreaterThan(Date.now() / 1000);
});

it("says so when the new window can't open", async () => {
  vi.spyOn(bridge.sessions, "attach").mockResolvedValue(false);
  expect(await useSessions.getState().attachDetached(session)).toBe(false);
  expect(useSessions.getState().openFailedUntil).toBeGreaterThan(Date.now() / 1000);
});
