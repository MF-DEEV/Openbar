// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What each agent's tile in the first-run window says (#168), as the Mac's
// OnboardingAgentsStep: the agents found connect by themselves while the step
// is open, the rest wait greyed out.

import type { HookAgent } from "../sessions/types";

export type TileState = "checking" | "connecting" | "connected" | "ready" | "missing" | "failed";

export interface ClaudeHooks {
  installed: boolean;
  error: string | null;
}

export type CodexStatus = "connected" | "notRunning" | "notFound";

export interface AgentHooks {
  agent: HookAgent;
  state: "installed" | "outdated" | "missing" | "unreadable";
  present: boolean;
  failure?: string;
}

/** Claude Code: its hooks go in from the window, once it is found. */
export function claudeTile(present: boolean | null, hooks: ClaudeHooks | null, failed: boolean): TileState {
  if (present === null || hooks === null) return "checking";
  if (hooks.installed) return "connected";
  if (failed) return "failed";
  return present ? "connecting" : "missing";
}

/** Codex needs nothing installed: its app-server is heard or not. */
export function codexTile(status: CodexStatus | null): TileState {
  switch (status) {
    case null:
      return "checking";
    case "connected":
      return "connected";
    case "notRunning":
      return "ready";
    case "notFound":
      return "missing";
  }
}

/** Gemini CLI, Copilot CLI, OpenCode and Cursor get their hooks from the service by themselves. */
export function hookAgentTile(status: AgentHooks | undefined, failed: boolean): TileState {
  if (!status) return "checking";
  if (failed || status.failure || status.state === "unreadable") return "failed";
  if (status.state === "installed") return "connected";
  return status.present ? "connecting" : "missing";
}

/** Whether Claude Code's hooks should be put in now: found, missing, not yet asked. */
export function shouldConnectClaude(present: boolean | null, hooks: ClaudeHooks | null, asked: boolean): boolean {
  return present === true && hooks !== null && !hooks.installed && !asked;
}

export type Step = "welcome" | "agents" | "tryIt" | "done";
export const STEPS: readonly Step[] = ["welcome", "agents", "tryIt", "done"];
