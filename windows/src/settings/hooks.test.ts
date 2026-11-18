// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { expect, it } from "vitest";
import {
  type HookRow,
  type HookState,
  failureText,
  folderFailureText,
  linkCaption,
  needsAttention,
  shownPath,
  stateText,
  summary,
  tone,
  toolFailureText,
} from "./hooks";

const row = (state: HookState): HookRow => ({
  id: JSON.stringify(state),
  provider: "gemini",
  name: "Gemini CLI",
  folder: "C:\\Users\\vors\\.gemini",
  file: "C:\\Users\\vors\\.gemini\\settings.json",
  wholeFile: false,
  manual: false,
  present: true,
  derived: "installed",
  state,
  canReinstall: true,
  canRemove: true,
});

it("words each state and the reason a file is skipped as the Mac does", () => {
  expect(stateText({ kind: "installed" })).toBe("Installed");
  expect(stateText({ kind: "outdated" })).toBe("Outdated: an older Openbar wrote these");
  expect(stateText({ kind: "changedByHand" })).toBe("Changed by hand");
  expect(stateText({ kind: "unreadable" })).toBe("Can’t read this file");
  expect(stateText({ kind: "skipped", reason: "folderMissing" })).toBe("Skipped: the config folder doesn’t exist yet");
  expect(stateText({ kind: "skipped", reason: "agentNotInstalled" })).toBe("Skipped: the agent isn’t installed");
  expect(stateText({ kind: "skipped", reason: "needsApproval" })).toBe(
    "Skipped: the hooks need approval inside the agent",
  );
  expect(stateText({ kind: "skipped", reason: "removedByYou" })).toBe("Skipped: you removed them");
});

it("counts only what the person can fix as needing a look", () => {
  expect(needsAttention({ kind: "skipped", reason: "needsApproval" })).toBe(true);
  expect(needsAttention({ kind: "missing" })).toBe(true);
  expect(needsAttention({ kind: "skipped", reason: "agentNotInstalled" })).toBe(false);
  expect(needsAttention({ kind: "installed" })).toBe(false);
  expect(tone({ kind: "installed" })).toBe("ok");
  expect(tone({ kind: "unreadable" })).toBe("error");
  expect(tone({ kind: "skipped", reason: "switchedOff" })).toBe("off");
  expect(tone({ kind: "skipped", reason: "needsApproval" })).toBe("warn");
});

it("sums the page up in a line for Settings › Sessions", () => {
  expect(linkCaption(7, 1)).toBe("7 configs work, 1 needs a look");
  expect(linkCaption(1, 0)).toBe("1 config works");
  expect(linkCaption(0, 3)).toBe("0 configs work, 3 need a look");
  const rows = [
    row({ kind: "installed" }),
    row({ kind: "installed" }),
    row({ kind: "changedByHand" }),
    row({ kind: "skipped", reason: "nothingNeeded" }),
  ];
  expect(summary(rows)).toEqual({ working: 2, attention: 1 });
});

it("writes paths the way each system does", () => {
  expect(shownPath("C:\\Users\\vors\\.claude\\settings.json", false)).toBe("C:\\Users\\vors\\.claude\\settings.json");
  expect(shownPath("/home/vors/.claude/settings.json", true)).toBe("~/.claude/settings.json");
  expect(shownPath("/root/.codex", true)).toBe("~/.codex");
  expect(shownPath("/home/vors", true)).toBe("~");
  expect(shownPath("/opt/tool/settings.json", true)).toBe("/opt/tool/settings.json");
  expect(shownPath("/homework/x", true)).toBe("/homework/x");
});

it("says why a change or an add failed", () => {
  expect(failureText("unreadable")).toMatch(/can’t be read/);
  expect(failureText("unwritable: Access is denied.")).toBe("Access is denied.");
  expect(failureText("something else")).toBe("something else");
  expect(folderFailureText("tooBroad")).toBe("Pick the agent’s config folder, not your whole home folder.");
  expect(folderFailureText("alreadyListed")).toBe("That folder is already on the page.");
  expect(toolFailureText("claudeAccount")).toBe("That file belongs to a Claude Code account, which is listed above.");
  expect(toolFailureText("tooMany")).toBe("Openbar keeps up to 8 tools.");
});
