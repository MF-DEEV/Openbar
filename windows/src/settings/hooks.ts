// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// What the Hooks page (#236) says about each config file, as the Mac's
// HooksPageStrings and HookTargets: the state and why a file is skipped,
// the line Settings › Sessions sums the page up in, and paths written the way
// this system writes them (hooks/page.rs keeps the rows).

import { type StringKey, t, tf } from "../i18n";
import { HOOK_AGENTS, type Provider } from "../sessions/types";

export type HookSkip =
  | "folderMissing"
  | "agentNotInstalled"
  | "needsApproval"
  | "switchedOff"
  | "accountHidden"
  | "nothingNeeded"
  | "removedByYou";

export type HookState =
  | { kind: "installed" | "outdated" | "changedByHand" | "missing" | "unreadable" }
  | { kind: "skipped"; reason: HookSkip };

export type HookDerived = "installed" | "outdated" | "changedByHand" | "removedByHand" | "missing" | "unreadable";

/** One config file the island writes its hooks into (hooks/page.rs `Target`). */
export interface HookRow {
  id: string;
  provider: Provider;
  /** The account, folder or tool name. */
  name: string;
  folder: string;
  /** None for Codex's own home, which needs no hooks. */
  file: string | null;
  wholeFile: boolean;
  /** Added by hand, so it can be taken off the page. */
  manual: boolean;
  /** The agent itself is installed. */
  present: boolean;
  derived: HookDerived;
  state: HookState;
  canReinstall: boolean;
  canRemove: boolean;
  failure?: string;
}

/** The page's cards, in the Mac's order. */
export const HOOK_PAGE_ORDER: readonly Provider[] = ["claude", "codex", ...HOOK_AGENTS];

/** The agents whose config folder can be added by hand, beside Claude Code's accounts. */
export const FOLDERS_BY_HAND: readonly Provider[] = ["codex", "gemini", "copilot", "pi", "vibe", "grok"];

const SKIPS: Record<HookSkip, StringKey> = {
  folderMissing: "windows.hooks.skipFolderMissing",
  agentNotInstalled: "windows.hooks.skipAgentNotInstalled",
  needsApproval: "windows.hooks.skipNeedsApproval",
  switchedOff: "windows.hooks.skipSwitchedOff",
  accountHidden: "windows.hooks.skipAccountHidden",
  nothingNeeded: "windows.hooks.skipNothingNeeded",
  removedByYou: "windows.hooks.skipRemovedByYou",
};

export function stateText(state: HookState): string {
  switch (state.kind) {
    case "installed":
      return t("windows.hooks.stateInstalled");
    case "outdated":
      return t("windows.hooks.stateOutdated");
    case "changedByHand":
      return t("windows.hooks.stateChanged");
    case "missing":
      return t("windows.hooks.stateMissing");
    case "unreadable":
      return t("windows.hooks.stateUnreadable");
    case "skipped":
      return tf("windows.hooks.skipped", t(SKIPS[state.reason]));
  }
}

/** Something the person may want to fix. */
export function needsAttention(state: HookState): boolean {
  if (state.kind === "skipped") return state.reason === "needsApproval";
  return state.kind !== "installed";
}

/** The dot beside a row: working, wants a look, broken, or left alone. */
export function tone(state: HookState): "ok" | "warn" | "error" | "off" {
  if (state.kind === "installed") return "ok";
  if (state.kind === "unreadable") return "error";
  if (state.kind === "skipped") return state.reason === "needsApproval" ? "warn" : "off";
  return "warn";
}

export function summary(rows: readonly HookRow[]): { working: number; attention: number } {
  return {
    working: rows.filter((r) => r.state.kind === "installed").length,
    attention: rows.filter((r) => needsAttention(r.state)).length,
  };
}

/** One line for Settings › Sessions: how many files work, how many want a look. */
export function linkCaption(working: number, attention: number): string {
  const files = working === 1 ? t("windows.hooks.linkWorksOne") : tf("windows.hooks.linkWorksMany", working);
  if (attention <= 0) return files;
  return (
    files + (attention === 1 ? t("windows.hooks.linkAttentionOne") : tf("windows.hooks.linkAttentionMany", attention))
  );
}

/** A path as this system writes it: Windows' full path with backslashes, a
 * Linux home's files as `~/…`. */
export function shownPath(path: string, linux: boolean): string {
  if (!linux) return path;
  const match = /^(?:\/home\/[^/]+|\/root)(\/.*)?$/.exec(path);
  return match ? "~" + (match[1] ?? "") : path;
}

/** Why a change failed, in words (hooks::installer::Failure as Rust says it). */
export function failureText(raw: string): string {
  if (raw === "unreadable") return t("notchClaudeSessions.settingsUnreadable");
  if (raw === "busy") return t("notchClaudeSessions.settingsBusy");
  if (raw === "noBackup") return t("notchClaudeSessions.settingsNoBackup");
  return raw.startsWith("unwritable: ") ? raw.slice("unwritable: ".length) : raw;
}

const FOLDER_FAILURES: Record<string, StringKey> = {
  notFolder: "windows.hooks.folderNotFolder",
  alreadyListed: "windows.hooks.folderAlreadyListed",
  tooMany: "windows.hooks.folderTooMany",
  tooBroad: "windows.hooks.folderTooBroad",
};

const TOOL_FAILURES: Record<string, StringKey> = {
  noName: "windows.hooks.toolNoName",
  notInFolder: "windows.hooks.toolNotInFolder",
  alreadyListed: "windows.hooks.toolAlreadyListed",
  tooMany: "windows.hooks.toolTooMany",
  claudeAccount: "windows.hooks.toolClaudeAccount",
};

/** Why a folder could not be added, from the code Rust gives. */
export function folderFailureText(code: string): string {
  const key = FOLDER_FAILURES[code];
  return key ? t(key) : failureText(code);
}

/** Why a tool could not be added, from the code Rust gives. */
export function toolFailureText(code: string): string {
  const key = TOOL_FAILURES[code];
  return key ? t(key) : failureText(code);
}
