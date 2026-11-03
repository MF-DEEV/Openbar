// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  began,
  CAP,
  compose,
  end,
  failed,
  finished,
  IDLE,
  partial,
  shortcutTarget,
  showsMicrophone,
  start,
  tick,
  type Machine,
} from "./voice";
import type { Session } from "./types";

const recording = (draft = "", sendOnStop = false): Machine => began(start(IDLE, "s", draft, sendOnStop)!, 100);

describe("voice replies", () => {
  it("starts, fills the field and keeps the last words on stop", () => {
    const preparing = start(IDLE, "s", "fix", false)!;
    expect(preparing.phase.kind).toBe("preparing");
    expect(start(preparing, "t", "", false)).toBeNull();
    let m = began(preparing, 10);
    expect(m.phase.kind).toBe("recording");
    let fx;
    [m, fx] = partial(m, "the tests");
    expect(fx.draft).toBe("fix the tests");
    [m, fx] = end(m, "stopped");
    expect(fx).toEqual({ finish: true });
    expect(m.phase).toEqual({ kind: "finishing", ending: "stopped" });
    expect(tick(m, 0, 999)[1]).toEqual({});
    [m, fx] = finished(m, "the tests please");
    expect(fx).toEqual({ draft: "fix the tests please" });
    expect(m.phase.kind).toBe("idle");
    expect(end(m, "stopped")[1]).toEqual({});
  });

  it("keeps the last partial when no final words come", () => {
    let m = recording();
    [m] = partial(m, "hello");
    [m] = end(m, "stopped");
    expect(finished(m, null)[1].draft).toBe("hello");
    expect(finished(m, "")[1].draft).toBe("hello");
  });

  it("gives the field back on Escape, at any point", () => {
    let m = recording("typed");
    [m] = partial(m, "spoken");
    const [after, fx] = end(m, "cancelled");
    expect(fx).toEqual({ abort: true, draft: "typed" });
    expect(after.phase.kind).toBe("idle");

    const preparing = start(IDLE, "s", "before", false)!;
    expect(end(preparing, "cancelled")[1]).toEqual({ abort: true, draft: "before" });

    let finishing = recording("a");
    [finishing] = partial(finishing, "b");
    [finishing] = end(finishing, "stopped");
    const [late, lateFx] = end(finishing, "cancelled");
    expect(lateFx).toEqual({ abort: true, draft: "a" });
    expect(finished(late, "b c")[1]).toEqual({});
  });

  it("ends on silence only after speech", () => {
    let m = recording();
    for (let i = 0; i < 40; i++) [m] = tick(m, 0.05, 100 + i * 0.1);
    expect(m.phase.kind).toBe("recording");
    [m] = tick(m, 0.6, 104.1);
    [m] = tick(m, 0.1, 104.2);
    let fx;
    [m, fx] = tick(m, 0.1, 105.6);
    expect(fx.finish).toBeUndefined();
    [m, fx] = tick(m, 0.1, 105.8);
    expect(fx.finish).toBe(true);
    expect(m.phase).toEqual({ kind: "finishing", ending: "silence" });

    let talking = recording();
    [talking] = tick(talking, 0.6, 100.1);
    [talking] = tick(talking, 0.1, 100.2);
    [talking] = partial(talking, "more");
    [talking] = tick(talking, 0.1, 101.3);
    expect(tick(talking, 0.1, 102.0)[1].finish).toBeUndefined();

    let middle = recording();
    [middle] = tick(middle, 0.6, 100.1);
    for (let i = 0; i < 30; i++) [middle] = tick(middle, 0.28, 100.2 + i * 0.1);
    expect(middle.phase.kind).toBe("recording");
  });

  it("caps a recording at 60 s and never sends it", () => {
    let m = recording("", true);
    [m] = partial(m, "a long thought");
    expect(tick(m, 0.6, 100 + CAP - 0.1)[1].finish).toBeUndefined();
    [m] = tick(m, 0.6, 100 + CAP);
    expect(m.phase).toEqual({ kind: "finishing", ending: "cap" });
    expect(finished(m, null)[1]).toEqual({ draft: "a long thought" });
  });

  it("drops everything when the session ends, keeps it when the island closes", () => {
    let gone = recording("x", true);
    [gone] = partial(gone, "words");
    const [idle, goneFx] = end(gone, "sessionGone");
    expect(goneFx).toEqual({ abort: true });
    expect(idle.phase.kind).toBe("idle");

    let closed = recording("x", true);
    [closed] = partial(closed, "words");
    let fx;
    [closed, fx] = end(closed, "islandClosed");
    expect(fx).toEqual({ finish: true });
    expect(finished(closed, "words here")[1]).toEqual({ draft: "x words here" });

    let finishing = recording();
    [finishing] = end(finishing, "stopped");
    const [after, lateFx] = end(finishing, "sessionGone");
    expect(lateFx.abort).toBe(true);
    expect(after.phase.kind).toBe("idle");
  });

  it("sends when I stop talking, and only then", () => {
    let stopped = recording("", true);
    [stopped] = partial(stopped, "run the tests");
    [stopped] = end(stopped, "stopped");
    expect(finished(stopped, "Run the tests.")[1].send).toBe("Run the tests.");

    let silent = recording("Also", true);
    [silent] = tick(silent, 0.6, 100.1);
    [silent] = partial(silent, "check lint");
    [silent] = tick(silent, 0.1, 100.3);
    [silent] = tick(silent, 0.1, 101.9);
    expect(finished(silent, null)[1].send).toBe("Also check lint");

    let nothing = recording("", true);
    [nothing] = end(nothing, "stopped");
    expect(finished(nothing, null)[1].send).toBeUndefined();

    let off = recording("", false);
    [off] = partial(off, "hello");
    [off] = end(off, "stopped");
    expect(finished(off, null)[1].send).toBeUndefined();
  });

  it("keeps what was heard when recognition fails", () => {
    const preparing = start(IDLE, "s", "typed", false)!;
    expect(failed(preparing)[1]).toEqual({ abort: true });
    let midway = recording("typed");
    [midway] = partial(midway, "half");
    const [idle, fx] = failed(midway);
    expect(fx).toEqual({ abort: true, draft: "typed half" });
    expect(partial(idle, "late")[1]).toEqual({});
  });

  it("joins words to the field with one space", () => {
    expect(compose("", "  hello ")).toBe("hello");
    expect(compose("fix", "it")).toBe("fix it");
    expect(compose("fix ", "it")).toBe("fix it");
    expect(compose("line\n", "next")).toBe("line\nnext");
    expect(compose("kept", "")).toBe("kept");
  });

  it("speaks to the page on screen, else the session last at work", () => {
    const session = (id: string, extra: Partial<Session> = {}) =>
      ({ id, acceptsReplies: true, phase: "idle", phaseSince: 0, lastActive: 0, ...extra }) as unknown as Session;
    const old = session("old", { lastActive: 10 });
    const recent = session("recent", { lastActive: 50 });
    const waiting = session("waiting", { phase: "waitingPermission", lastActive: 90 } as Partial<Session>);
    const unreachable = session("warp", { acceptsReplies: false, lastActive: 99 });
    const ended = session("ended", { background: { ended: true } } as unknown as Partial<Session>);
    const all = [old, recent, waiting, unreachable, ended];
    expect(shortcutTarget(all, "old")?.id).toBe("old");
    expect(shortcutTarget(all, null)?.id).toBe("recent");
    expect(shortcutTarget(all, "waiting")).toBeNull();
    expect(shortcutTarget([waiting, unreachable], null)).toBeNull();
    expect(shortcutTarget(all, "gone")?.id).toBe("recent");
    expect(showsMicrophone(unreachable, true)).toBe(false);
    expect(showsMicrophone(ended, true)).toBe(false);
    expect(showsMicrophone(recent, true)).toBe(true);
    expect(showsMicrophone(recent, false)).toBe(false);
  });
});
