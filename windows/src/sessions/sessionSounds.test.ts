// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Session } from "./types";
import { DEFAULT_SOUND_PREFS, muteUntil, resolveSessionSounds, soundsMuted } from "./soundPreferences";
import { SessionSoundEvents, SessionSoundQueue, terminalSound, type SessionSoundEvent } from "./waitingSound";
import { previewSound } from "../settings/SessionSounds";

const row = (id: string, phase: Session["phase"], extra: Partial<Session> = {}) =>
  ({ id, provider: "claude", phase, phaseSince: 5, turnStarted: 2, ...extra }) as Session;
const request = (id: string, questions: unknown[] = []) =>
  ({ requestId: id, questions, since: 5 }) as Session["permission"];

describe("agent lifecycle sounds", () => {
  it("maps permission, question, completion, error, limit and start to six distinct events", () => {
    const detector = new SessionSoundEvents(0);
    detector.arrived([], {});
    const started = row("a", "generating", { startedAt: 1 });
    expect(detector.arrived([started], {}).map((e) => e.kind)).toEqual(["started"]);
    const permission = row("a", "waitingPermission", { startedAt: 1, permission: request("p1") });
    expect(detector.arrived([permission], {}).map((e) => e.kind)).toEqual(["permission"]);
    expect(detector.arrived([permission], {})).toEqual([]);
    const question = row("a", "waitingInput", {
      startedAt: 1,
      phaseSince: 6,
      permission: request("q1", [{ question: "Continue?" }]),
    });
    expect(detector.arrived([question], {}).map((e) => e.kind)).toEqual(["question"]);
    const done = row("a", "finished", { startedAt: 1, turnEndedAt: 7, turnCompleted: true });
    expect(detector.arrived([done], {}).map((e) => e.kind)).toEqual(["finished"]);
    const error = row("a", "idle", { startedAt: 1, turnStarted: 8, turnEndedAt: 9, failedAt: 9, turnCompleted: false });
    expect(detector.arrived([error], {}).map((e) => e.kind)).toEqual(["error"]);
    const limit = row("a", "limited", {
      startedAt: 1,
      turnStarted: 10,
      turnEndedAt: 11,
      failedAt: 11,
      turnCompleted: false,
      limitStop: { at: 11 },
    });
    expect(detector.arrived([limit], {}).map((e) => e.kind)).toEqual(["limit"]);
    expect(detector.arrived([limit], {})).toEqual([]);
  });

  it("never treats a recoverable tool failure as a terminal error", () => {
    const recovering = row("a", "generating", { timeline: [{ id: "tool", failed: true }] as Session["timeline"] });
    expect(terminalSound(recovering)).toBeNull();
    expect(terminalSound(row("a", "idle", { timeline: recovering.timeline, turnCompleted: false }))).toBeNull();
    expect(terminalSound(row("a", "idle", { failedAt: 9, turnEndedAt: 9 }))?.kind).toBe("error");
    expect(terminalSound(row("a", "limited", { failedAt: 9, turnEndedAt: 9, limitStop: { at: 9 } }))?.kind).toBe(
      "limit",
    );
  });

  it("uses only parent rows, retaining parent requests from subagents", () => {
    const detector = new SessionSoundEvents(0);
    detector.arrived([], {});
    const parent = row("parent", "waitingPermission", {
      permission: request("child-request"),
      subagents: [{ id: "child" }] as unknown as Session["subagents"],
    });
    expect(detector.arrived([parent], {}).map((e) => e.session)).toEqual(["parent"]);
  });

  it("does not replay restored sessions or reconnected requests", () => {
    const detector = new SessionSoundEvents(0);
    const old = row("a", "waitingPermission", { startedAt: 1, permission: request("p") });
    expect(detector.arrived([old], {})).toEqual([]);
    detector.arrived([], {});
    expect(detector.arrived([old], {})).toEqual([]);
    expect(
      detector
        .arrived([row("a", "waitingPermission", { phaseSince: 7, permission: request("p2") })], {})
        .map((e) => e.kind),
    ).toEqual(["permission"]);
  });

  it("isolates simultaneous sessions and permits a truly new request", () => {
    const detector = new SessionSoundEvents(0);
    detector.arrived([], {});
    const first = [
      row("a", "waitingPermission", { permission: request("r") }),
      row("b", "waitingPermission", { permission: request("r") }),
    ];
    expect(detector.arrived(first, {}).map((e) => e.session)).toEqual(["a", "b"]);
    expect(
      detector
        .arrived([row("a", "waitingPermission", { permission: request("r2") }), first[1]], {})
        .map((e) => e.session),
    ).toEqual(["a"]);
  });
});

describe("sound preferences and mute", () => {
  it("migrates #167 without writing or resetting old choices", () => {
    expect(resolveSessionSounds({ waitingSoundChoice: "Windows Ding.wav" })).toMatchObject({
      soundPermission: "Windows Ding.wav",
      soundQuestion: "Windows Ding.wav",
      soundStarted: "none",
      soundFinished: "none",
      soundError: "none",
      soundLimit: "none",
    });
    expect(resolveSessionSounds({ waitingSound: false, waitingSoundChoice: "Glass" })).toMatchObject({
      soundPermission: "none",
      soundQuestion: "none",
      soundFinished: "none",
    });
    expect(
      resolveSessionSounds({ waitingSound: false, soundPermission: "openbar:soft", soundQuestion: "none" }),
    ).toMatchObject({ soundPermission: "openbar:soft", soundQuestion: "none" });
    expect(
      resolveSessionSounds(JSON.parse(JSON.stringify(resolveSessionSounds({ soundVolume: 34 })))).soundVolume,
    ).toBe(34);
  });
  it("defaults to no new-event noise and clamps invalid volume", () => {
    expect(DEFAULT_SOUND_PREFS.soundStarted).toBe("none");
    expect(resolveSessionSounds({ soundVolume: 250 }).soundVolume).toBe(100);
    expect(resolveSessionSounds({ soundVolume: -2 }).soundVolume).toBe(0);
    expect(resolveSessionSounds({ soundVolume: NaN }).soundVolume).toBe(100);
  });
  it("uses local midnight including daylight saving transitions and restores after expiry", () => {
    const now = new Date(2026, 10, 15, 17, 30);
    const until = muteUntil("tomorrow", now);
    expect(new Date(until * 1000).getDate()).toBe(16);
    expect(new Date(until * 1000).getHours()).toBe(0);
    expect(muteUntil("hour", now) - now.getTime() / 1000).toBe(3600);
    expect(soundsMuted({ ...DEFAULT_SOUND_PREFS, soundMutedUntil: until }, now.getTime() / 1000)).toBe(true);
    expect(soundsMuted({ ...DEFAULT_SOUND_PREFS, soundMutedUntil: until }, until)).toBe(false);
    expect(soundsMuted({ ...DEFAULT_SOUND_PREFS, soundMutedUntil: 0 }, now.getTime() / 1000)).toBe(false);
  });
});

describe("sound playback coordination", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  const event = (session: string, kind: SessionSoundEvent["kind"], at = 10): SessionSoundEvent => ({
    session,
    kind,
    id: String(at),
    at,
  });
  const selected = {
    ...DEFAULT_SOUND_PREFS,
    soundPermission: "a",
    soundQuestion: "b",
    soundFinished: "c",
    soundError: "d",
    soundLimit: "e",
    soundStarted: "f",
  };
  function rig() {
    let prefs = { ...selected };
    let focused: string | null = null;
    let terminalFocused: string | null = null;
    const played = vi.fn(async (_sound: string, _volume: number) => true);
    const queue = new SessionSoundQueue(
      () => ({ prefs, enabled: true, inView: (id) => id === focused, current: () => true }),
      async (ids) => ids[0] === terminalFocused,
      played,
      () => 10,
    );
    return {
      queue,
      played,
      setPrefs: (next: typeof prefs) => {
        prefs = next;
      },
      focus: (id: string | null) => {
        focused = id;
      },
      terminal: (id: string | null) => {
        terminalFocused = id;
      },
    };
  }
  for (const [kind, sound] of Object.entries({
    permission: "a",
    question: "b",
    finished: "c",
    error: "d",
    limit: "e",
    started: "f",
  }) as [SessionSoundEvent["kind"], string][]) {
    it(`${kind} plays its own selected sound and volume`, async () => {
      const { queue, played } = rig();
      queue.enqueue([event("a", kind)]);
      await vi.advanceTimersByTimeAsync(1000);
      expect(played).toHaveBeenCalledExactlyOnceWith(sound, 100);
      queue.dispose();
    });
  }
  it("None, zero volume, and temporary mute silence every event", async () => {
    const r = rig();
    r.setPrefs({ ...selected, soundQuestion: "none" });
    r.queue.enqueue([event("a", "question")]);
    r.setPrefs({ ...selected, soundVolume: 0 });
    r.queue.enqueue([event("a", "permission")]);
    r.setPrefs({ ...selected, soundMutedUntil: 11 });
    r.queue.enqueue([event("a", "limit")]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.played).not.toHaveBeenCalled();
    r.setPrefs({ ...selected, soundVolume: 27 });
    r.queue.enqueue([event("a", "permission")]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.played).toHaveBeenCalledExactlyOnceWith("a", 27);
    r.queue.dispose();
  });
  it("one-second bursts play one highest-priority sound without an error double", async () => {
    const r = rig();
    r.queue.enqueue([event("a", "permission"), event("b", "error"), event("c", "limit")]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.played).toHaveBeenCalledExactlyOnceWith("e", 100);
    r.queue.dispose();
  });
  it("suppresses exactly the viewed session or focused terminal tab", async () => {
    const r = rig();
    r.focus("a");
    r.terminal("b");
    r.queue.enqueue([event("a", "limit"), event("b", "error"), event("c", "question")]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.played).toHaveBeenCalledExactlyOnceWith("b", 100);
    r.queue.dispose();
  });
  it("checks volume and focus again after an asynchronous terminal response", async () => {
    let prefs = { ...selected };
    let respond: ((shown: boolean) => void) | undefined;
    const played = vi.fn();
    const queue = new SessionSoundQueue(
      () => ({ prefs, enabled: true, inView: () => false, current: () => true }),
      () =>
        new Promise((resolve) => {
          respond = resolve;
        }),
      played,
      () => 10,
    );
    queue.enqueue([event("a", "question")]);
    await vi.advanceTimersByTimeAsync(1000);
    prefs = { ...prefs, soundVolume: 0 };
    respond?.(false);
    await Promise.resolve();
    await Promise.resolve();
    expect(played).not.toHaveBeenCalled();
    queue.dispose();
  });
  it("audio failures do not break future session notifications", async () => {
    const r = rig();
    r.played.mockRejectedValueOnce(new Error("no device"));
    r.queue.enqueue([event("a", "error")]);
    await vi.advanceTimersByTimeAsync(1000);
    r.queue.enqueue([event("a", "question", 11)]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.played).toHaveBeenCalledTimes(2);
    r.queue.dispose();
  });
  it("preview respects chosen volume, never changes events, and skips None", async () => {
    const play = vi.fn(async () => true);
    expect(await previewSound("openbar:alert", 42, play)).toBe(true);
    expect(play).toHaveBeenCalledExactlyOnceWith("openbar:alert", 42);
    expect(await previewSound("none", 42, play)).toBe(true);
    expect(await previewSound("openbar:soft", 0, play)).toBe(true);
    expect(play).toHaveBeenCalledTimes(1);
  });
});
