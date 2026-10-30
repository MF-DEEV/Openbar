// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The person's preferences as every window sees them. The Rust side keeps
// only what was changed (prefs.rs); the defaults live here, so a value never
// set follows the defaults if they ever change.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { MODULES, type ModuleId } from "../island/modules";
import {
  CLIPBOARD_LIMITS,
  CLIPBOARD_RETENTION_DAYS,
  DEFAULT_SETTINGS,
  type IslandSettings,
  type QuickButton,
} from "../island/settings";
import { inTauri } from "./bridge";
import { MONITOR_METRICS, type MonitorMetric } from "../system/types";
import { CONTROL_ITEMS, type ControlItem } from "../controls/types";
import { AGENT_CARDS, type AgentCard, type CodexProfile } from "../agents/types";

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
  idleContent: ["none", "battery", "music", "agents"],
  // "all" is a Mac choice the Windows island doesn't have yet.
  display: ["primary", "cursor"],
  timerMode: ["timer", "pomodoro", "stopwatch"],
  networkSpeedUnit: ["bytes", "bits"],
  colorPickerFormat: ["hex", "rgb", "hsl"],
  scratchpadRetention: ["never", "day", "week", "month"],
  clipboardCardSize: ["compact", "comfortable"],
  agentsPeriod: ["today", "week", "month"],
  agentsLimitDisplay: ["remaining", "used"],
  agentsLimitFocus: ["mostUsed", "session", "weekly"],
  agentsReadout: ["elapsed", "tokens", "cost", "limit"],
  watchCondition: ["changes", "settles", "contains", "reaches"],
};

/** Lists of ids or names: strings only, each once. */
function stringList(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return [...new Set(value.filter((v): v is string => typeof v === "string" && v.trim().length > 0))];
}

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

/** Known home-page items, each once. */
function controlList(value: unknown): ControlItem[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const known = new Set<string>(CONTROL_ITEMS);
  return [...new Set(value.filter((id): id is ControlItem => typeof id === "string" && known.has(id)))];
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** At most eight homes (CodexAccountProfile.maximum). */
export const CODEX_PROFILES_MAXIMUM = 8;
export const CODEX_PROFILE_NAME_MAXIMUM = 40;
/** One folder spelled with other slashes or case is the same folder. */
export const codexHomeKey = (directory: string) => directory.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();

/** White space by either JavaScript's or Rust's reckoning (agent_usage::profiles::is_blank). */
export const isBlank = (name: string) => /^[\s\u0085]*$/.test(name);

/** Saved Codex homes as the usage service reads them (agent_usage::profiles::decode). */
function codexProfiles(value: unknown): CodexProfile[] {
  if (!Array.isArray(value)) return [];
  const ids = new Set<string>();
  const paths = new Set<string>();
  return value.slice(0, CODEX_PROFILES_MAXIMUM).filter((p): p is CodexProfile => {
    if (!isObject(p) || typeof p.id !== "string" || typeof p.name !== "string" || typeof p.directory !== "string")
      return false;
    const id = p.id.toLowerCase();
    const path = codexHomeKey(p.directory);
    if (
      !UUID.test(p.id) ||
      isBlank(p.name) ||
      [...p.name].length > CODEX_PROFILE_NAME_MAXIMUM ||
      !/^[a-z]:[\\/]/i.test(p.directory) ||
      ids.has(id) ||
      paths.has(path)
    )
      return false;
    ids.add(id);
    paths.add(path);
    return true;
  });
}

const strings = (value: unknown): string[] =>
  Array.isArray(value) ? [...new Set(value.filter((v): v is string => typeof v === "string" && v.length > 0))] : [];

/** The mixer's pinned apps and order; anything else reads as none. */
function arrangement(value: unknown): IslandSettings["mixerAppArrangement"] {
  if (!isObject(value)) return DEFAULT_SETTINGS.mixerAppArrangement;
  return { order: strings(value.order), pinned: strings(value.pinned) };
}

function hiddenApps(value: unknown): Record<string, string> {
  if (!isObject(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter(([key, name]) => key.length > 0 && typeof name === "string"),
  ) as Record<string, string>;
}

export function resolve(stored: StoredPrefs | null | undefined): Prefs {
  const island = checked(DEFAULT_SETTINGS, stored?.island);
  const raw = isObject(stored?.island) ? stored.island : {};
  if (
    island.preferredPlayer !== undefined &&
    (island.preferredPlayer.length > 256 || /[\x00-\x1f]/.test(island.preferredPlayer))
  )
    delete island.preferredPlayer;
  if (island.screenshotPreviewDuration !== undefined)
    island.screenshotPreviewDuration = Math.min(300, Math.max(0, Math.round(island.screenshotPreviewDuration)));
  const quickAccess = quickButtons(raw.quickAccess);
  const hiddenModules = moduleList(raw.hiddenModules);
  const moduleOrder = moduleList(raw.moduleOrder);
  const controlOrder = controlList(raw.controlOrder);
  const hiddenControls = controlList(raw.hiddenControls);
  const agentsCardOrder = cardList(raw.agentsCardOrder);
  const agentsHiddenCards = cardList(raw.agentsHiddenCards);
  // Clamped as the Mac reads them (NotchAgentSupport.finishMinimum, limitThreshold, dailyBudget).
  if (island.agentsFinishMinimum !== undefined)
    island.agentsFinishMinimum = Math.min(3600, Math.max(0, island.agentsFinishMinimum));
  if (island.agentsLimitThreshold !== undefined)
    island.agentsLimitThreshold = Math.min(100, Math.max(1, island.agentsLimitThreshold));
  if (island.agentsDailyBudget !== undefined) island.agentsDailyBudget = Math.max(0, island.agentsDailyBudget);
  if (island.mixerHeadphonesDisconnectVolumePercent !== undefined)
    island.mixerHeadphonesDisconnectVolumePercent = Math.min(
      100,
      Math.max(0, Math.round(island.mixerHeadphonesDisconnectVolumePercent)),
    );
  if (island.soundOutputSwitcherShortcut !== undefined && island.soundOutputSwitcherShortcut.trim() === "")
    delete island.soundOutputSwitcherShortcut;
  // At least one agent stays read, as the usage service treats none (agent_usage::settings).
  const agents = ["agentsClaude", "agentsCodex", "agentsOpenCode", "agentsCopilot"] as const;
  if (agents.every((key) => island[key] === false)) agents.forEach((key) => delete island[key]);
  if (island.homeModule !== undefined && island.homeModule !== "explore" && !KNOWN.has(island.homeModule)) {
    delete island.homeModule;
  }
  // The Mac's choices only; any other number reads as the default.
  if (island.clipboardHistoryLimit !== undefined && !CLIPBOARD_LIMITS.includes(island.clipboardHistoryLimit))
    delete island.clipboardHistoryLimit;
  if (
    island.clipboardHistoryRetentionDays !== undefined &&
    !CLIPBOARD_RETENTION_DAYS.includes(island.clipboardHistoryRetentionDays)
  )
    delete island.clipboardHistoryRetentionDays;
  return {
    island: {
      ...DEFAULT_SETTINGS,
      ...island,
      ...(hiddenModules ? { hiddenModules } : {}),
      ...(moduleOrder ? { moduleOrder } : {}),
      ...(quickAccess ? { quickAccess } : {}),
      ...(controlOrder ? { controlOrder } : {}),
      ...(hiddenControls ? { hiddenControls } : {}),
      mixerAppArrangement: arrangement(raw.mixerAppArrangement),
      mixerHiddenApps: hiddenApps(raw.mixerHiddenApps),
      ...(agentsCardOrder ? { agentsCardOrder } : {}),
      ...(agentsHiddenCards ? { agentsHiddenCards } : {}),
      agentsCodexProfiles: codexProfiles(raw.agentsCodexProfiles).map(({ id, name, directory }) => ({
        id,
        name,
        directory,
      })),
      downloadFolders: Array.isArray(raw.downloadFolders)
        ? raw.downloadFolders.filter((v): v is string => typeof v === "string" && v.length > 0)
        : [],
      ...Object.fromEntries(
        (
          [
            "toolsOrder",
            "toolsHidden",
            "togglesOrder",
            "togglesHidden",
            "ejectExcluded",
            "soundOutputSwitcherDevices",
          ] as const
        ).map((key) => [key, stringList(raw[key]) ?? []]),
      ),
      clipboardIgnoredApps: Array.isArray(raw.clipboardIgnoredApps)
        ? raw.clipboardIgnoredApps.filter((v): v is string => typeof v === "string" && v.trim().length > 0)
        : [],
      capsuleFit: { ...DEFAULT_SETTINGS.capsuleFit, ...checked(DEFAULT_SETTINGS.capsuleFit, raw.capsuleFit) },
      calendarExcluded: Array.isArray(raw.calendarExcluded)
        ? raw.calendarExcluded.filter((v): v is string => typeof v === "string")
        : [],
      calendarChosenCountdowns: isObject(raw.calendarChosenCountdowns)
        ? Object.fromEntries(
            Object.entries(raw.calendarChosenCountdowns).filter(
              (entry): entry is [string, number] => typeof entry[1] === "number" && Number.isFinite(entry[1]),
            ),
          )
        : {},
      sections: { ...DEFAULT_SETTINGS.sections, ...sectionSwitches(raw.sections) },
      monitorInterval: [1, 2, 5].includes(raw.monitorInterval as number)
        ? (raw.monitorInterval as number)
        : DEFAULT_SETTINGS.monitorInterval,
      hiddenSystemMetrics: Array.isArray(raw.hiddenSystemMetrics)
        ? [
            ...new Set(
              raw.hiddenSystemMetrics.filter((m): m is MonitorMetric => MONITOR_METRICS.includes(m as MonitorMetric)),
            ),
          ]
        : [],
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
