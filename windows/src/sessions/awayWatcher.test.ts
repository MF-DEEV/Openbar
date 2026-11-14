import { beforeEach, describe, expect, it, vi } from "vitest";

const handlers: Record<string, () => void> = {};
let idle: number | null = 0;
vi.mock("../shared/bridge", () => ({
  bridge: {
    session: {
      onLock: (h: () => void) => ((handlers.lock = h), Promise.resolve(() => {})),
      onUnlock: (h: () => void) => ((handlers.unlock = h), Promise.resolve(() => {})),
      idleSeconds: () => Promise.resolve(idle),
    },
  },
}));
const presented: unknown[] = [];
vi.mock("./progress/AwayCard", () => ({ presentAway: (...args: unknown[]) => presented.push(args) }));
vi.mock("../island/store", () => ({ useIsland: { subscribe: () => () => {} } }));

const { useSessions } = await import("./store");
const { awayState, startAwayWatcher } = await import("./awayWatcher");

function snapshot(added: number, touched: number[]) {
  const session = {
    id: "s",
    provider: "claude",
    pid: 1,
    cwd: "C:\\\\p",
    project: "p",
    name: "p",
    host: "windowsTerminal",
    acceptsReplies: true,
    phase: "generating",
    phaseSince: 0,
    timeline: [],
    subagents: [],
    tokens: { input: 0, cacheWrite: 0, cacheRead: 0, output: 0 },
    cost: 0,
    hooked: true,
    chat: [],
    services: 0,
    edits: { added, removed: 0, touched },
  };
  return { sessions: [session], counts: { working: 1, waiting: 0, finished: 0 }, previews: true, hooksInstalled: true };
}

describe("away watcher (#215)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    presented.length = 0;
  });

  it("a lock ends only with the unlock, whatever the input reading says", async () => {
    // Launch: the watcher starts before the first snapshot.
    useSessions.getState().receive({ ...snapshot(0, []), sessions: [] } as never);
    startAwayWatcher();
    // The first snapshot after launch joins the mark as it stands.
    useSessions.getState().receive(snapshot(50, [1]) as never);
    expect(awayState.seen?.sessions.s?.added).toBe(50);

    handlers.lock!();
    expect(awayState.away).not.toBeNull();
    // Input just before the lock still reads as recent: no return while locked.
    idle = 2;
    await vi.advanceTimersByTimeAsync(25_000);
    expect(awayState.away).not.toBeNull();

    useSessions.getState().receive(snapshot(70, [1, Date.now() / 1000 + 5]) as never);
    handlers.unlock!();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(awayState.away).toBeNull();
    expect(presented).toHaveLength(1);
    const [lines] = presented[0] as [{ added: number; files: number }[]];
    expect(lines[0]).toMatchObject({ added: 20, files: 1 });
  });
});
