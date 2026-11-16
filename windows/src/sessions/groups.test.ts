// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import { branchLabel, COLLAPSED_LIMIT, cutMiddle, groupSessions, toggleCollapsed } from "./groups";
import { orderSessions } from "./SessionsList";
import type { Session } from "./types";

const base = demoSnapshot().sessions[0]!;
const at = (over: Partial<Session> & Pick<Session, "id">): Session => ({
  ...base,
  permission: undefined,
  phase: "idle",
  remote: undefined,
  ...over,
});

describe("groupSessions", () => {
  const list = orderSessions([
    at({ id: "1", project: "web", provider: "claude", phase: "generating", turnStarted: 10 }),
    at({ id: "2", project: "api", provider: "codex", phase: "finished", phaseSince: 50 }),
    at({ id: "3", project: "api", provider: "claude", phase: "waitingPermission", phaseSince: 40 }),
    at({ id: "4", project: "web", provider: "gemini", phase: "idle", lastActive: 5 }),
    at({ id: "5", project: "api", provider: "claude", remote: "devbox", phase: "generating", turnStarted: 20 }),
  ]);

  it("puts a group with someone waiting on top and keeps the list's order inside", () => {
    const groups = groupSessions(list, "project");
    expect(groups.map((g) => [g.title, g.remote ?? "", g.sessions.map((s) => s.id)])).toEqual([
      ["api", "", ["3", "2"]],
      ["web", "", ["1", "4"]],
      ["api", "devbox", ["5"]],
    ]);
    expect(groups.map((g) => g.waiting)).toEqual([1, 0, 0]);
  });

  it("keeps a project on another machine apart, under its host", () => {
    const keys = groupSessions(list, "project").map((g) => g.key);
    expect(new Set(keys).size).toBe(3);
    expect(keys).toContain("project:devbox:api");
  });

  it("groups by agent under the agent's own name", () => {
    const groups = groupSessions(list, "agent");
    expect(groups.map((g) => [g.title, g.provider, g.sessions.length])).toEqual([
      ["Claude Code", "claude", 3],
      // Done before quiet, as in the list.
      ["Codex", "codex", 1],
      ["Gemini CLI", "gemini", 1],
    ]);
  });

  it("orders groups after the list, so None and groups agree on who comes first", () => {
    const flat = groupSessions(list, "agent").flatMap((g) => g.sessions);
    expect(flat[0]!.id).toBe(list[0]!.id);
  });
});

describe("toggleCollapsed", () => {
  it("folds and opens a group", () => {
    expect(toggleCollapsed([], "a")).toEqual(["a"]);
    expect(toggleCollapsed(["a", "b"], "a")).toEqual(["b"]);
  });

  it("keeps only the latest folds", () => {
    const many = Array.from({ length: COLLAPSED_LIMIT }, (_, i) => `k${i}`);
    const next = toggleCollapsed(many, "new");
    expect(next).toHaveLength(COLLAPSED_LIMIT);
    expect(next.at(-1)).toBe("new");
    expect(next).not.toContain("k0");
  });
});

describe("branchLabel", () => {
  it("shows the branch alone outside a worktree", () => {
    expect(branchLabel({ branch: "feat/login", cwd: "C:\\repo" })).toEqual({
      branch: "feat/login",
      worktree: null,
      inWorktree: false,
      full: "feat/login",
    });
  });

  it("shows nothing on a detached HEAD or outside git", () => {
    expect(branchLabel({ branch: undefined, cwd: "C:\\repo" })).toBeNull();
    expect(branchLabel({ branch: "  ", cwd: "/home/me/notes" })).toBeNull();
  });

  it("names a worktree once when its branch already carries the name", () => {
    expect(branchLabel({ branch: "worktree-fix-login", cwd: "/r/.claude/worktrees/fix-login" })).toMatchObject({
      branch: "worktree-fix-login",
      worktree: null,
      inWorktree: true,
    });
    expect(branchLabel({ branch: "openbar/docs", cwd: "C:\\r\\.openbar\\worktrees\\docs" })).toMatchObject({
      worktree: null,
      inWorktree: true,
    });
  });

  it("names the worktree and the branch when the session checked out another branch there", () => {
    expect(branchLabel({ branch: "main", cwd: "/r/.claude/worktrees/fix-login/src" })).toEqual({
      branch: "main",
      worktree: "fix-login",
      inWorktree: true,
      full: "fix-login · main",
    });
  });

  it("names a detached worktree by its folder", () => {
    expect(branchLabel({ branch: undefined, cwd: "/r/.openbar/worktrees/spike" })).toMatchObject({
      branch: null,
      worktree: "spike",
      full: "spike",
    });
  });

  it("tells two worktrees of one repository apart", () => {
    const a = branchLabel({ branch: "worktree-a", cwd: "/r/.claude/worktrees/a" });
    const b = branchLabel({ branch: "worktree-b", cwd: "/r/.claude/worktrees/b" });
    expect(a!.full).not.toBe(b!.full);
  });
});

describe("cutMiddle", () => {
  it("leaves a short name whole", () => {
    expect(cutMiddle("main", 26)).toBe("main");
    expect(cutMiddle("a".repeat(26), 26)).toBe("a".repeat(26));
  });

  it("keeps the start and the end of a long name", () => {
    const cut = cutMiddle("feat/currency-rounding-for-the-new-checkout-totals", 26);
    expect([...cut]).toHaveLength(26);
    expect(cut.startsWith("feat/currency")).toBe(true);
    expect(cut.endsWith("ckout-totals")).toBe(true);
    expect(cut).toContain("…");
  });

  it("counts characters, not UTF-16 units", () => {
    const cut = cutMiddle("feat/😀😀😀😀😀😀😀😀😀😀", 7);
    expect([...cut]).toHaveLength(7);
  });
});
