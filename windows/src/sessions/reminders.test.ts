// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import { describe, expect, it, vi } from "vitest";
import { DEFAULT_SESSION_PREFS } from "../shared/prefs";
import {
  ReminderRunner,
  SessionReminders,
  heldByDnd,
  reminderSettings,
  reminderSound,
  reminderWait,
  waitedMinutes,
  type ReminderDeps,
  type ReminderSettings,
} from "./reminders";
import type { Permission, Session } from "./types";

const at = 100_000;

function session(over: Partial<Session> & Pick<Session, "id" | "phase">): Session {
  return {
    provider: "claude",
    pid: 1,
    cwd: "C:\\p",
    project: "p",
    name: over.id,
    host: "windowsTerminal",
    acceptsReplies: true,
    phaseSince: at,
    timeline: [],
    subagents: [],
    tokens: { input: 0, cacheWrite: 0, cacheRead: 0, output: 0 },
    cost: 0,
    hooked: true,
    chat: [],
    services: 0,
    ...over,
  };
}

const permission = (requestId: string, since = at, questions: Permission["questions"] = []): Permission => ({
  requestId,
  tool: "Bash",
  summary: "ls",
  detail: "ls",
  offersAlways: false,
  since,
  questions,
  fitsOnALine: true,
});

const all: ReminderSettings = {
  permission: true,
  question: true,
  finished: true,
  after: 300,
  every: null,
  duringDnd: false,
};

describe("session reminders (#231): when one is due", () => {
  const waiting = [session({ id: "a", phase: "waitingPermission", permission: permission("r1") })];

  it("is off by default", () => {
    const off = reminderSettings(DEFAULT_SESSION_PREFS);
    expect(off).toEqual({ ...all, permission: false, question: false, finished: false });
    expect(new SessionReminders().due(waiting, off, at + 3600)).toEqual([]);
  });

  it("comes once at the time", () => {
    const reminders = new SessionReminders();
    expect(reminders.due(waiting, all, at + 299)).toEqual([]);
    const due = reminders.due(waiting, all, at + 300);
    expect(due.map((w) => [w.session, w.kind])).toEqual([["a", "permission"]]);
    reminders.mark(due, at + 300);
    expect(reminders.due(waiting, all, at + 3000)).toEqual([]);
  });

  it("has a switch for each kind, and tells questions apart", () => {
    const questions = { ...all, permission: false };
    const reminders = new SessionReminders();
    expect(reminders.due(waiting, questions, at + 600)).toEqual([]);
    expect(reminders.due([session({ id: "q", phase: "waitingInput" })], questions, at + 600)[0]?.kind).toBe("question");
    const asked = session({
      id: "p",
      phase: "waitingPermission",
      permission: permission("r2", at, [{ question: "Which?", header: "", options: [], multiSelect: false }]),
    });
    expect(reminders.due([asked], questions, at + 600)[0]?.kind).toBe("question");
  });

  it("reads minutes from Settings", () => {
    const on = reminderSettings({ ...DEFAULT_SESSION_PREFS, remindQuestion: true, remindAfter: 2, remindEvery: 10 });
    expect(on.question && !on.permission && on.after === 120 && on.every === 600).toBe(true);
  });
});

describe("session reminders (#231): waits answered or seen", () => {
  it("starts over for the next request after one answered in the terminal", () => {
    const reminders = new SessionReminders();
    const first = [session({ id: "a", phase: "waitingPermission", permission: permission("r1") })];
    reminders.mark(reminders.due(first, all, at + 300), at + 300);
    const next = [
      session({ id: "a", phase: "waitingPermission", phaseSince: at + 400, permission: permission("r2", at + 400) }),
    ];
    expect(reminders.due(next, all, at + 600)).toEqual([]);
    expect(reminders.due(next, all, at + 700).map((w) => w.session)).toEqual(["a"]);
  });

  it("keeps a prompt the hook names later as the same wait", () => {
    const reminders = new SessionReminders();
    reminders.mark(reminders.due([session({ id: "b", phase: "waitingPermission" })], all, at + 300), at + 300);
    const named = [session({ id: "b", phase: "waitingPermission", permission: permission("r9") })];
    expect(reminders.due(named, all, at + 900)).toEqual([]);
  });

  it("tells whether a wait is still current", () => {
    const wait = reminderWait(session({ id: "a", phase: "waitingInput" }), at)!;
    expect(SessionReminders.current(wait, [session({ id: "a", phase: "waitingInput" })], at)).toBe(true);
    expect(SessionReminders.current(wait, [session({ id: "a", phase: "generating" })], at)).toBe(false);
    expect(SessionReminders.current(wait, [], at)).toBe(false);
  });
});

describe("session reminders (#231): finished turns", () => {
  const done = session({ id: "f", phase: "finished", turnStarted: at - 100, turnEndedAt: at, turnCompleted: true });

  it("reminds of a turn nobody looked at, also once the phase went idle", () => {
    expect(new SessionReminders().due([done], all, at + 300)[0]?.kind).toBe("finished");
    expect(new SessionReminders().due([{ ...done, phase: "idle" }], all, at + 900)[0]?.kind).toBe("finished");
  });

  it("lets go of looked at, failed, interrupted, old or followed turns", () => {
    expect(reminderWait({ ...done, lookedAt: at + 10 }, at + 300)).toBeNull();
    expect(reminderWait({ ...done, failedAt: at }, at + 300)).toBeNull();
    expect(reminderWait({ ...done, turnCompleted: false }, at + 300)).toBeNull();
    expect(reminderWait(done, at + 3600)).toBeNull();
    expect(reminderWait({ ...done, phase: "generating", turnStarted: at + 50 }, at + 300)).toBeNull();
  });
});

describe("session reminders (#231): one for many, repeats, sound", () => {
  const three = [
    session({ id: "a", phase: "waitingPermission", permission: permission("r1") }),
    session({ id: "b", phase: "waitingInput", phaseSince: at + 60 }),
    session({ id: "c", phase: "waitingPermission", phaseSince: at + 120, permission: permission("r3", at + 120) }),
    session({ id: "late", phase: "waitingInput", phaseSince: at + 250 }),
  ];

  it("gives sessions waiting together one reminder", () => {
    const reminders = new SessionReminders();
    const due = reminders.due(three, all, at + 300);
    expect(due.map((w) => w.session)).toEqual(["a", "b", "c"]);
    reminders.mark(due, at + 300);
    expect(reminders.due(three, all, at + 549)).toEqual([]);
    expect(reminders.due(three, all, at + 550).map((w) => w.session)).toEqual(["late"]);
    expect(reminderSound(due)).toBe("permission");
    expect(reminderSound(due.filter((w) => w.kind === "question"))).toBe("question");
    expect(reminderSound([])).toBeNull();
  });

  it("comes again every N minutes when asked to", () => {
    const every = { ...all, every: 600 };
    const one = [three[0]];
    const reminders = new SessionReminders();
    reminders.mark(reminders.due(one, every, at + 300), at + 300);
    expect(reminders.due(one, every, at + 899)).toEqual([]);
    expect(reminders.due(one, every, at + 900).map((w) => w.session)).toEqual(["a"]);
  });
});

describe("session reminders (#231): row mark and Do Not Disturb", () => {
  it("reads whole minutes from one on", () => {
    const waiting = session({ id: "a", phase: "waitingPermission", permission: permission("r1", at - 30) });
    expect(waitedMinutes(waiting, at + 20)).toBeNull();
    expect(waitedMinutes(waiting, at + 690)).toBe(12);
    expect(waitedMinutes(session({ id: "i", phase: "idle" }), at + 690)).toBeNull();
  });

  it("holds for Do Not Disturb unless allowed, and never for an unknown one", () => {
    expect(heldByDnd(all, true)).toBe(true);
    expect(heldByDnd(all, null)).toBe(false);
    expect(heldByDnd({ ...all, duringDnd: true }, true)).toBe(false);
  });
});

describe("session reminders (#231): the runner", () => {
  const prefs = {
    ...DEFAULT_SESSION_PREFS,
    remindPermission: true,
    remindQuestion: true,
    remindFinished: true,
    remindAfter: 5,
  };

  function runner(over: Partial<ReminderDeps> & { rows?: Session[]; time?: number } = {}) {
    let rows = over.rows ?? [
      session({ id: "a", phase: "waitingPermission", permission: permission("r1") }),
      session({ id: "b", phase: "waitingInput", phaseSince: at + 30 }),
      session({ id: "c", phase: "waitingInput", phaseSince: at + 60 }),
    ];
    const play = vi.fn();
    const present = vi.fn();
    const deps: ReminderDeps = {
      sessions: () => rows,
      prefs: () => prefs,
      enabled: () => true,
      quiet: async () => ({ locked: false, dnd: false }),
      locked: () => false,
      terminalShown: async () => false,
      play,
      present,
      now: () => over.time ?? at + 300,
      ...over,
    };
    return { run: new ReminderRunner(deps), play, present, setRows: (next: Session[]) => (rows = next) };
  }

  it("gives three waiting sessions one reminder and one sound", async () => {
    const { run, play, present } = runner();
    expect((await run.check()).map((w) => w.session)).toEqual(["a", "b", "c"]);
    expect(present).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalledWith("", 100);
    expect(await run.check()).toEqual([]);
    expect(present).toHaveBeenCalledTimes(1);
  });

  it("waits while locked or the display is off, and comes after", async () => {
    let locked = true;
    const { run, present } = runner({ quiet: async () => ({ locked, dnd: false }) });
    expect(await run.check()).toEqual([]);
    locked = false;
    expect(await run.check()).toHaveLength(3);
    expect(present).toHaveBeenCalledTimes(1);
    let away = true;
    const heard = runner({ locked: () => away });
    expect(await heard.run.check()).toEqual([]);
    away = false;
    expect(await heard.run.check()).toHaveLength(3);
  });

  it("waits for Do Not Disturb unless allowed", async () => {
    const held = runner({ quiet: async () => ({ locked: false, dnd: true }) });
    expect(await held.run.check()).toEqual([]);
    const allowed = runner({
      quiet: async () => ({ locked: false, dnd: true }),
      prefs: () => ({ ...prefs, remindDuringDnd: true }),
    });
    expect(await allowed.run.check()).toHaveLength(3);
  });

  it("counts a session whose own terminal tab is in front as seen", async () => {
    const { run, present } = runner({ terminalShown: async (ids) => ids[0] === "a" });
    expect((await run.check()).map((w) => w.session)).toEqual(["b", "c"]);
    expect(present).toHaveBeenCalledTimes(1);
    expect(await run.check()).toEqual([]);
    const all = runner({ terminalShown: async () => true });
    expect(await all.run.check()).toEqual([]);
    expect(all.present).not.toHaveBeenCalled();
  });

  it("drops a wait answered while the terminals were asked", async () => {
    const box: { setRows?: (rows: Session[]) => void } = {};
    const { run, present, setRows } = runner({
      terminalShown: async () => {
        box.setRows?.([session({ id: "a", phase: "generating" })]);
        return false;
      },
    });
    box.setRows = setRows;
    expect(await run.check()).toEqual([]);
    expect(present).not.toHaveBeenCalled();
  });

  it("stays quiet when muted, at zero volume or set to none, and still opens", async () => {
    const muted = runner({ prefs: () => ({ ...prefs, soundMutedUntil: at + 9999 }) });
    await muted.run.check();
    expect(muted.play).not.toHaveBeenCalled();
    expect(muted.present).toHaveBeenCalledTimes(1);
    const none = runner({ prefs: () => ({ ...prefs, soundPermission: "none" }) });
    await none.run.check();
    expect(none.play).not.toHaveBeenCalled();
  });

  it("does nothing with the section off or before the time", async () => {
    const off = runner({ enabled: () => false });
    expect(await off.run.check()).toEqual([]);
    const early = runner({ time: at + 200 });
    expect(await early.run.check()).toEqual([]);
  });
});
