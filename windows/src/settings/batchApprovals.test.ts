// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import { expect, it } from "vitest";
import { groupBatches, shownDetail } from "./BatchApprovals";
import type { BatchApproval } from "../sessions/types";

const entry = (batch: string, at: number, detail: string): BatchApproval => ({
  batch,
  at,
  sessionId: "s",
  session: "s",
  project: "p",
  agent: "claude",
  tool: "Bash",
  detail,
});

it("groups the asks of one click and keeps the list's order", () => {
  const groups = groupBatches([entry("b2", 20, "ls"), entry("b2", 20, "pwd"), entry("b1", 10, "git status")]);
  expect(groups.map((g) => [g.id, g.at, g.entries.map((e) => e.detail)])).toEqual([
    ["b2", 20, ["ls", "pwd"]],
    ["b1", 10, ["git status"]],
  ]);
  expect(groupBatches([])).toEqual([]);
});

it("leaves out a detail that only names the listed files by shorter paths", () => {
  const edit = { ...entry("b", 1, "src\\cart.ts\nold.ts"), paths: ["C:\\shop\\src\\cart.ts", "/shop/old.ts"] };
  expect(shownDetail(edit)).toBeNull();
  expect(shownDetail({ ...edit, detail: "2 files" })).toBe("2 files");
  expect(shownDetail(entry("b", 1, "ls"))).toBe("ls");
  expect(shownDetail(entry("b", 1, "  "))).toBeNull();
});
