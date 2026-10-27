// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The person's preferences as every window sees them. The Rust side keeps
// only what was changed (prefs.rs); the defaults live here, so a value never
// set follows the defaults if they ever change.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { MODULES, type ModuleId } from "../island/modules";
import { DEFAULT_SETTINGS, type IslandSettings, type QuickButton } from "../island/settings";
import { inTauri } from "./bridge";
import { AGENT_CARDS, type AgentCard } from "../agents/types";

/** The Sessions section's own preferences (spec 01-shell §7.2, sessions). */
export interface SessionPrefs {
  /** Show what sessions say and run; off keeps only names and phases. */
  previews: boolean;
  /** A session that needs the person opens the island on its page. */
  autoExpand: boolean;
  /** Sessions show beside the camera while they work. */
  live: boolean;
  /** Each session's pixel buddy; off shows a round phase badge. */
  buddies: boolean;
}

export const DEFAULT_SESSION_PREFS: SessionPrefs = {
  previews: false,
  autoExpand: true,
  live: true,
  buddies: true,
};

/** What is stored: only what differs from the defaults. */
export interface StoredPrefs {
  island?: Partial<IslandSettings>;
  sessions?: Partial<SessionPrefs>;
}

export interface Prefs {
  island: IslandSettings;
  sessions: SessionPrefs;
}

/** A patch: objects merge, anything else replaces, null puts the default back. */
export type PrefsPatch = {
  island?: { [K in keyof IslandSettings]?: IslandSettings[K] | null };
  sessions?: { [K in keyof SessionPrefs]?: SessionPrefs[K] | null };
};

/** Values settings.json may hold for the enumerated keys; anything else reads as the default. */
const CHOICES: Partial<Record<keyof IslandSettings, readonly string[]>> = {
  silhouette: ["notch", "capsule"],
  layout: ["compact", "spacious", "custom"],
  opening: ["click", "hoverPreview", "hoverExpand", "hiddenUntilHover"],
  // "all" is a Mac choice the Windows island doesn't have yet.
  display: ["primary", "cursor"],
  agentsPeriod: ["today", "week", "month"],
  agentsLimitDisplay: ["remaining", "used"],
  agentsLimitFocus: ["mostUsed", "session", "weekly"],
  agentsReadout: ["elapsed", "tokens", "cost", "limit"],
};

const KNOWN = new Set<string>(MODULES.map((m) => m.id));

/** Known section ids, each once. */
function moduleList(value: unknown): ModuleId[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return [...new Set(value.filter((id): id is ModuleId => typeof id === "string" && KNOWN.has(id)))];
}

/** Known AI page cards, each once. */
function cardList(value: unknown): AgentCard[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return [...new Set(value.filter((id): id is AgentCard => AGENT_CARDS.includes(id as AgentCard)))];
}

/** Takes from `stored` only what has the default's type (and a known value), so a hand-edited file can't break a window. */
function checked<T extends object>(defaults: T, stored: unknown): Partial<T> {
  if (!isObject(stored)) return {};
  const out: Partial<T> = {};
  for (const key of Object.keys(defaults) as (keyof T & string)[]) {
    const value = stored[key];
    const fallback = defaults[key];
    if (value === undefined || value === null) continue;
    if (Array.isArray(fallback) ? !Array.isArray(value) : typeof value !== typeof fallback) continue;
    if (typeof value === "number" && !Number.isFinite(value)) continue;
    const choices = CHOICES[key as keyof IslandSettings];
    if (choices && !choices.includes(value as string)) continue;
    out[key] = value as T[keyof T & string];
  }
  return out;
}

/** Buttons around the island that every reader can draw: a known side and action. */
function quickButtons(value: unknown): QuickButton[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter((b): b is QuickButton => {
    if (!isObject(b) || typeof b.id !== "string" || !["left", "right", "bottom"].includes(b.side as string))
      return false;
    if (b.label !== undefined && typeof b.label !== "string") return false;
    const action = b.action;
    if (!isObject(action)) return false;
    if (action.kind === "module") return typeof action.module === "string" && KNOWN.has(action.module);
    return action.kind === "explore" || action.kind === "settings" || action.kind === "pin";
  });
}

export function resolve(stored: StoredPrefs | null | undefined): Prefs {
  const island = checked(DEFAULT_SETTINGS, stored?.island);
  const raw = isObject(stored?.island) ? stored.island : {};
  const quickAccess = quickButtons(raw.quickAccess);
  const hiddenModules = moduleList(raw.hiddenModules);
  const moduleOrder = moduleList(raw.moduleOrder);
  const agentsCardOrder = cardList(raw.agentsCardOrder);
  const agentsHiddenCards = cardList(raw.agentsHiddenCards);
  // Clamped as the Mac reads them (NotchAgentSupport.finishMinimum, limitThreshold, dailyBudget).
  if (island.agentsFinishMinimum !== undefined)
    island.agentsFinishMinimum = Math.min(3600, Math.max(0, island.agentsFinishMinimum));
  if (island.agentsLimitThreshold !== undefined)
    island.agentsLimitThreshold = Math.min(100, Math.max(1, island.agentsLimitThreshold));
  if (island.agentsDailyBudget !== undefined) island.agentsDailyBudget = Math.max(0, island.agentsDailyBudget);
  if (island.homeModule !== undefined && island.homeModule !== "explore" && !KNOWN.has(island.homeModule)) {
    delete island.homeModule;
  }
  return {
    island: {
      ...DEFAULT_SETTINGS,
      ...island,
      ...(hiddenModules ? { hiddenModules } : {}),
      ...(moduleOrder ? { moduleOrder } : {}),
      ...(quickAccess ? { quickAccess } : {}),
      ...(agentsCardOrder ? { agentsCardOrder } : {}),
      ...(agentsHiddenCards ? { agentsHiddenCards } : {}),
      downloadFolders: Array.isArray(raw.downloadFolders)
        ? raw.downloadFolders.filter((v): v is string => typeof v === "string" && v.length > 0)
        : [],
      capsuleFit: { ...DEFAULT_SETTINGS.capsuleFit, ...checked(DEFAULT_SETTINGS.capsuleFit, raw.capsuleFit) },
      sections: { ...DEFAULT_SETTINGS.sections, ...sectionSwitches(raw.sections) },
    },
    sessions: { ...DEFAULT_SESSION_PREFS, ...checked(DEFAULT_SESSION_PREFS, stored?.sessions) },
  };
}

function sectionSwitches(value: unknown): Partial<Record<ModuleId, boolean>> {
  if (!isObject(value)) return {};
  return Object.fromEntries(Object.entries(value).filter(([id, on]) => KNOWN.has(id) && typeof on === "boolean"));
}

// Outside Tauri (designing in a browser) the preferences live in memory.
let memory: StoredPrefs = {};
const memoryListeners = new Set<(p: Prefs) => void>();

function mergeInto(target: Record<string, unknown>, patch: Record<string, unknown>) {
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) delete target[key];
    else if (isObject(value)) {
      // A new object is merged into nothing, so its nulls drop out (as in prefs.rs).
      if (!isObject(target[key])) target[key] = {};
      mergeInto(target[key] as Record<string, unknown>, value);
    } else target[key] = Array.isArray(value) ? structuredClone(value) : value;
  }
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

export async function loadPrefs(): Promise<Prefs> {
  if (!inTauri) return resolve(memory);
  return resolve(await invoke<StoredPrefs>("prefs_get"));
}

export async function setPrefs(patch: PrefsPatch): Promise<Prefs> {
  if (!inTauri) {
    mergeInto(memory as Record<string, unknown>, patch as Record<string, unknown>);
    const next = resolve(memory);
    memoryListeners.forEach((l) => l(next));
    return next;
  }
  return resolve(await invoke<StoredPrefs>("prefs_set", { patch }));
}

export function onPrefs(handler: (p: Prefs) => void): Promise<UnlistenFn> {
  if (!inTauri) {
    memoryListeners.add(handler);
    return Promise.resolve(() => memoryListeners.delete(handler));
  }
  return listen<StoredPrefs>("prefs://changed", (e) => handler(resolve(e.payload)));
}

export const launchAtLogin = {
  get: async () => (inTauri ? invoke<boolean>("launch_at_login_get") : false),
  set: async (on: boolean) => (inTauri ? invoke<boolean>("launch_at_login_set", { on }) : on),
};

export { mergeInto as mergePatch };
