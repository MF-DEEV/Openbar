// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { tf } from "../i18n/index";

// How sessions read in the island (NotchClaudeSessionsSupport.swift,
// NotchAgentSupport.swift; spec 02-sessions §4).

import { S } from "./strings";
import { isWaiting, isWorking, type Phase, type Provider, type Session, type Tokens } from "./types";

export const CLAUDE_TINT = "#D97857";
/** Codex's periwinkle, the same as the Mac's agent tint. */
export const CODEX_TINT = "#7D99FF";
export const WAITING_TINT = "#FFC747";
export const FINISHED_TINT = "#5CD685";
export const IDLE_TINT = "rgba(255,255,255,0.45)";
export const LINK = "#6BA8FF";

const PROJECT_PALETTE = ["#6BA3FF", "#B88CFF", "#FF80BD", "#45D1D6", "#9ED65C", "#D6AD85", "#61DBFF", "#DBC7FF"];

export function providerTint(provider: Provider): string {
  return provider === "codex" ? CODEX_TINT : CLAUDE_TINT;
}

/** Working phases take the agent's own color, the rest are shared. */
export function phaseTint(phase: Phase, provider: Provider = "claude"): string {
  switch (phase) {
    case "generating":
    case "runningTool":
      return providerTint(provider);
    case "background":
      return tf("windows.sessions.colorMixInSrgbValue75Transparent", providerTint(provider));
    case "waitingPermission":
    case "waitingInput":
      return WAITING_TINT;
    case "finished":
      return FINISHED_TINT;
    case "idle":
      return IDLE_TINT;
  }
}

export function phaseLabel(phase: Phase): string {
  return S[phase];
}

/** FNV-1a 64 over the lowercased project's UTF-8, mod 8: the same color as on the Mac. */
export function projectColor(project: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(project.toLowerCase())) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return PROJECT_PALETTE[Number(hash % 8n)]!;
}

/** The island's own name, else the registry name set by /rename, else the title, name, project. */
export function displayName(session: Session, custom?: string): string {
  if (custom) return custom;
  if (session.nameSource === "user" && session.name) return session.name;
  return session.title || session.name || session.project;
}

/** m:ss or h:mm:ss, never negative. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** The row's clock: since the turn began while working, since the phase began while waiting or in background. */
export function phaseClock(session: Session, now: number): string | null {
  if (isWorking(session.phase)) return clock(now - (session.turnStarted ?? session.phaseSince));
  if (isWaiting(session.phase) || session.phase === "background") return clock(now - session.phaseSince);
  return null;
}

/** "Done after 4m 12s". */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  if (h > 0) return tf("windows.sessions.valueHValueM", h, m);
  if (m > 0) return tf("windows.sessions.valueMValueS", m, r);
  return `${r}s`;
}

export function totalTokens(t: Tokens): number {
  return t.input + t.cacheWrite + t.cacheRead + t.output;
}

/** n, 4.2K, 48K, 1.2M: one decimal below 10, floored. */
export function compactNumber(n: number): string {
  const units: [number, string][] = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ];
  for (const [size, unit] of units) {
    if (n >= size) {
      const v = n / size;
      return v < 10 ? `${Math.floor(v * 10) / 10}${unit}` : `${Math.floor(v)}${unit}`;
    }
  }
  return String(Math.floor(n));
}

/** $0.42, $123, $12K. */
export function cost(value: number): string {
  if (value >= 1000) return `$${compactNumber(value)}`;
  if (value >= 100) return `$${Math.round(value)}`;
  return `$${value.toFixed(2)}`;
}

/** What a row says after its phase (NotchClaudeSessionsView.activityText). */
export function activityText(session: Session, name: string): string {
  const parts: (string | undefined)[] = [phaseLabel(session.phase)];
  const p = session.permission;
  switch (session.phase) {
    case "waitingPermission":
      parts.push(p?.tool ?? session.currentTool?.name, p ? (p.fitsOnALine ? p.detail : p.summary) : undefined);
      break;
    case "runningTool": {
      const t = session.currentTool;
      parts.push(t ? (t.summary ? `${t.name}: ${t.summary}` : t.name) : undefined);
      break;
    }
    case "waitingInput":
      parts.push(p?.summary ?? (session.title !== name ? session.title : undefined));
      break;
    case "generating":
      parts.push(session.title !== name ? session.title : undefined);
      break;
    case "background":
      parts.push(session.backgroundLabel ?? (session.title !== name ? session.title : undefined));
      if (session.cost > 0) parts.push(cost(session.cost));
      break;
    case "finished":
    case "idle":
      parts.push(session.title !== name ? session.title : undefined);
      if (session.cost > 0) parts.push(cost(session.cost));
      break;
  }
  if (session.services > 0 && !isWorking(session.phase)) parts.push(S.leftRunning(session.services));
  if (session.tasks && session.tasks.total > 0 && isWorking(session.phase)) {
    parts.push(`${session.tasks.done}/${session.tasks.total}`);
  }
  return parts.filter((x): x is string => !!x && x.length > 0).join(" · ");
}

/** Idle for five minutes since it last did anything or was looked at: the Clawd dozes. */
export function dozing(session: Session, now: number, lookedAt?: number): boolean {
  if (session.phase !== "idle") return false;
  const last = Math.max(session.lastActive ?? session.phaseSince, session.lookedAt ?? 0, lookedAt ?? 0);
  return now - last >= 300;
}

const THINKING = [
  "Thinking",
  "Pondering",
  "Brewing",
  "Musing",
  "Noodling",
  "Percolating",
  "Simmering",
  "Tinkering",
  "Mulling",
  "Crafting",
  "Conjuring",
  "Pontificating",
  "Ruminating",
  "Churning",
  "Whirring",
  "Cogitating",
];

/** One word per turn, seeded like the Mac (seconds since 2001-01-01) so both show the same. */
export function thinkingWord(turnStarted: number): string {
  const index = ((Math.floor(turnStarted - 978307200) % 16) + 16) % 16;
  return `${THINKING[index]}…`;
}
