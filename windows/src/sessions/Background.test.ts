// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  backgroundPhase,
  canRemoveBackground,
  canStopBackground,
  displayName,
  problemText,
  sessionPhaseLabel,
} from "./format";
import { S } from "./strings";
import type { BackgroundDetails, Phase, Session } from "./types";

function session(phase: Phase, background?: Partial<BackgroundDetails>): Session {
  return {
    id: "s",
    phase,
    name: "folder",
    project: "folder",
    background: background ? { job: "j", ended: false, ...background } : undefined,
  } as unknown as Session;
}

describe("background sessions", () => {
  it("a terminal session has no background actions", () => {
    const plain = session("idle");
    expect(canStopBackground(plain)).toBe(false);
    expect(canRemoveBackground(plain)).toBe(false);
    expect(backgroundPhase(plain)).toBeNull();
    expect(sessionPhaseLabel(plain)).toBe(S.idle);
  });

  it("stops what runs and removes what no longer works", () => {
    const working = session("generating", { state: "working" });
    expect(canStopBackground(working)).toBe(true);
    expect(canRemoveBackground(working)).toBe(false);
    const done = session("idle", { state: "done" });
    expect(canStopBackground(done)).toBe(true);
    expect(canRemoveBackground(done)).toBe(true);
    const gone = session("idle", { state: "stopped", ended: true });
    expect(canStopBackground(gone)).toBe(false);
    expect(canRemoveBackground(gone)).toBe(true);
    expect(canRemoveBackground(session("waitingPermission", { state: "blocked" }))).toBe(false);
  });

  it("says what Agent View says where the phase doesn't", () => {
    expect(backgroundPhase(session("idle", { state: "blocked" }))).toBe(S.backgroundNeedsReply);
    expect(backgroundPhase(session("waitingPermission", { state: "blocked" }))).toBeNull();
    expect(backgroundPhase(session("idle", { ended: true }))).toBe(S.backgroundStopped);
    expect(backgroundPhase(session("idle", { ended: true, state: "failed" }))).toBe(S.backgroundFailed);
    expect(backgroundPhase(session("idle", { ended: true, state: "done" }))).toBe(S.finished);
  });

  it("goes by Agent View's name, under the person's own", () => {
    const named = { ...session("idle", {}), name: "clarify-x", nameSource: "peer", title: "A title" };
    expect(displayName(named)).toBe("clarify-x");
    expect(displayName(named, "Mine")).toBe("Mine");
  });

  it("says what went wrong in words", () => {
    expect(problemText(null)).toBeNull();
    expect(problemText(undefined)).toBe(S.commandFailed);
    expect(problemText("unavailable")).toBe(S.commandFailed);
    expect(problemText("notTrusted")).toBe(S.folderNotTrusted);
    expect(problemText("No job matching 'x'")).toBe("No job matching 'x'");
  });
});
