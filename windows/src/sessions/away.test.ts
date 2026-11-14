import { describe, expect, it } from "vitest";
import { awayLines, awayMark, withNewSessions } from "./away";
import type { Session } from "./types";

const at = 1000;

function session(over: Partial<Session> & Pick<Session, "id" | "phase">): Session {
  return {
    provider: "claude",
    pid: 1,
    cwd: "C:\\p",
    project: "p",
    name: "p",
    host: "windowsTerminal",
    acceptsReplies: true,
    phaseSince: at - 100,
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

describe("away card lines (#215)", () => {
  it("shows nothing when nothing changed", () => {
    const sessions = [session({ id: "a", phase: "idle" }), session({ id: "b", phase: "waitingInput" })];
    expect(awayLines(awayMark(sessions, at), sessions)).toEqual([]);
  });

  it("names what finished, waits, stopped or failed, waiting first", () => {
    const before = [
      session({ id: "done", phase: "generating" }),
      session({ id: "wait", phase: "runningTool" }),
      session({ id: "lim", phase: "generating" }),
      session({ id: "err", phase: "generating" }),
    ];
    const after = [
      session({ id: "done", phase: "finished", phaseSince: at + 50, lastActive: at + 50 }),
      session({ id: "wait", phase: "waitingPermission", phaseSince: at + 60 }),
      session({ id: "lim", phase: "limited", phaseSince: at + 70 }),
      session({ id: "err", phase: "idle", phaseSince: at + 80, failedAt: at + 80 }),
    ];
    const lines = awayLines(awayMark(before, at), after);
    expect(lines.map((l) => [l.id, l.outcome])).toEqual([
      ["wait", "waiting"],
      ["err", "error"],
      ["lim", "limited"],
      ["done", "finished"],
    ]);
  });

  it("counts only files and lines changed while away", () => {
    const before = [
      session({
        id: "w",
        phase: "generating",
        edits: { added: 10, removed: 2, touched: [at - 50] },
        tasks: { done: 1, total: 5 },
      }),
    ];
    const after = [
      session({
        id: "w",
        phase: "runningTool",
        edits: { added: 40, removed: 5, touched: [at - 50, at + 10, at + 20] },
        tasks: { done: 4, total: 5 },
      }),
    ];
    const [line] = awayLines(awayMark(before, at), after);
    expect(line).toEqual({
      id: "w",
      outcome: "working",
      files: 2,
      added: 30,
      removed: 3,
      tasks: { from: 1, to: 4, total: 5 },
    });
  });

  it("leaves out a session still at work that moved nothing", () => {
    const sessions = [session({ id: "w", phase: "generating", tasks: { done: 2, total: 5 } })];
    expect(awayLines(awayMark(sessions, at), sessions)).toEqual([]);
  });

  it("counts a session begun and finished while away", () => {
    const after = [
      session({ id: "n", phase: "finished", turnStarted: at + 5, phaseSince: at + 90, lastActive: at + 90 }),
    ];
    expect(awayLines(awayMark([], at), after).map((l) => l.outcome)).toEqual(["finished"]);
  });

  it("counts only what a session that showed up later did after it showed up", () => {
    const early = awayMark([], at - 60);
    const seen = withNewSessions(early, [
      session({ id: "s", phase: "generating", edits: { added: 300, removed: 100, touched: [at - 500] } }),
    ]);
    const after = [
      session({ id: "s", phase: "generating", edits: { added: 310, removed: 102, touched: [at - 500, at + 5] } }),
    ];
    const [line] = awayLines(seen, after);
    expect([line?.added, line?.removed, line?.files]).toEqual([10, 2, 1]);
    expect(withNewSessions(seen, after)).toBe(seen);
  });
});
