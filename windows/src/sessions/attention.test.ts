// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it, vi } from "vitest";
import { DeferredSessionAttention, noticeLeavesWhenBusy } from "./attention";
import { demoSnapshot } from "./demo";
import type { SessionEvent } from "./types";
const event: SessionEvent = { kind: "needsPermission", id: "fixture", tool: "Shell" };
describe("session attention", () => {
  it("finds an existing prompt from snapshots without a new permission event", async () => {
    let allowed = false;
    const present = vi.fn();
    const attention = new DeferredSessionAttention(
      async () => allowed,
      () => "request1",
      present,
    );
    await attention.sync([event]);
    expect(present).not.toHaveBeenCalled();
    allowed = true;
    await attention.sync([event]);
    await attention.sync([event]);
    expect(present).toHaveBeenCalledExactlyOnceWith(event);
  });
  it("does not reopen a dismissed desktop prompt on each polling snapshot, but shows it after game return", async () => {
    let allowed = true;
    const present = vi.fn();
    const attention = new DeferredSessionAttention(
      async () => allowed,
      () => "request1",
      present,
    );
    await attention.sync([event]);
    await attention.handle(event);
    await attention.sync([event]);
    expect(present).toHaveBeenCalledTimes(1);
    allowed = false;
    await attention.sync([event]);
    allowed = true;
    await attention.sync([event]);
    expect(present).toHaveBeenCalledTimes(2);
  });
  it("keeps game prompts queued and presents a still-pending prompt once on desktop return", async () => {
    let allowed = false;
    const present = vi.fn();
    const attention = new DeferredSessionAttention(
      async () => allowed,
      () => "request1",
      present,
    );
    await attention.handle(event);
    await attention.resume();
    expect(present).not.toHaveBeenCalled();
    allowed = true;
    await attention.resume();
    await attention.resume();
    expect(present).toHaveBeenCalledExactlyOnceWith(event);
  });
  it("discards requests answered elsewhere while playing", async () => {
    let request: string | undefined = "request1";
    const present = vi.fn();
    let allowed = false;
    const attention = new DeferredSessionAttention(
      async () => allowed,
      () => request,
      present,
    );
    await attention.handle(event);
    request = undefined;
    allowed = true;
    await attention.resume();
    expect(present).not.toHaveBeenCalled();
  });
  it("does not replay an old approval when a different request replaced it", async () => {
    let request = "request1";
    const present = vi.fn();
    const attention = new DeferredSessionAttention(
      async () => false,
      () => request,
      present,
    );
    await attention.handle(event);
    request = "request2";
    await attention.resume();
    expect(present).not.toHaveBeenCalled();
  });
  it("drops completion banners while busy and fails closed if foreground detection fails", async () => {
    const present = vi.fn();
    const attention = new DeferredSessionAttention(
      async () => {
        throw new Error("unavailable");
      },
      () => "request",
      present,
    );
    await attention.handle({ kind: "finished", id: "fixture" });
    await attention.handle(event);
    await attention.resume();
    expect(present).not.toHaveBeenCalled();
  });
});

describe("session notices when another app is in use", () => {
  const sessions = demoSnapshot().sessions;
  const waiting = sessions.find((s) => s.phase === "waitingPermission")!;
  const finished = sessions.find((s) => s.phase === "finished")!;
  it("removes a notice about a session that still waits", () => {
    expect(noticeLeavesWhenBusy(waiting.id, sessions)).toBe(true);
  });
  it("keeps a finished turn's notice, and one whose session has gone", () => {
    expect(noticeLeavesWhenBusy(finished.id, sessions)).toBe(false);
    expect(noticeLeavesWhenBusy("gone", sessions)).toBe(false);
  });
  it("removes a notice that names no session", () => {
    expect(noticeLeavesWhenBusy(undefined, sessions)).toBe(true);
  });
});
