// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { problemText } from "./format";
import { S } from "./strings";
import { isOwnWorktree, worktreeName, worktreeSlug } from "./worktree";

describe("session worktrees", () => {
  it("names a worktree from the prompt as the Mac and the service do", () => {
    expect(worktreeSlug("Fix the login bug!")).toBe("fix-the-login-bug");
    expect(worktreeSlug("  Zażółć gęślą jaźń  ")).toBe("zazolc-gesla-jazn");
    expect(worktreeSlug("Straße Øre")).toBe("strasse-ore");
    expect(worktreeSlug("日本語だけ")).toBe("");
    expect(worktreeSlug("Please refactor the payments worker so retries back off exponentially")).toBe(
      "please-refactor-the-payments-worker-so",
    );
    expect(worktreeSlug("a".repeat(60))).toHaveLength(40);
    expect(worktreeSlug("--rm -rf /")).toBe("rm-rf");
  });

  it("finds the worktree a session works in", () => {
    expect(worktreeName("C:\\code\\app\\.openbar\\worktrees\\fix-login\\web")).toBe("fix-login");
    expect(worktreeName("/Users/me/app/.claude/worktrees/fix")).toBe("fix");
    expect(worktreeName("C:/code/app/.gemini/worktrees/x")).toBe("x");
    expect(worktreeName("C:\\code\\app")).toBeNull();
    expect(worktreeName("C:\\code\\app\\.openbar\\worktrees\\")).toBeNull();
    expect(worktreeName(undefined)).toBeNull();
    expect(isOwnWorktree("C:\\r\\.openbar\\worktrees\\fix")).toBe(true);
    expect(isOwnWorktree("C:\\r\\.claude\\worktrees\\fix")).toBe(false);
  });

  it("words a worktree that couldn't be made", () => {
    expect(problemText("worktreeFailed")).toBe(S.worktreeFailed);
    expect(problemText("worktreeFailed: fatal: invalid reference: HEAD")).toBe(
      `${S.worktreeFailed}: fatal: invalid reference: HEAD`,
    );
  });
});
