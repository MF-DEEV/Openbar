// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The person's preferences as every window sees them. The Rust side keeps
// only what was changed (prefs.rs); the defaults live here, so a value never
// set follows the defaults if they ever change.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { DEFAULT_SETTINGS, type IslandSettings } from "../island/settings";
import { inTauri } from "./bridge";

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

export function resolve(stored: StoredPrefs | null | undefined): Prefs {
  const island = stored?.island ?? {};
  return {
    island: {
      ...DEFAULT_SETTINGS,
      ...island,
      capsuleFit: { ...DEFAULT_SETTINGS.capsuleFit, ...island.capsuleFit },
      sections: { ...DEFAULT_SETTINGS.sections, ...island.sections },
    },
    sessions: { ...DEFAULT_SESSION_PREFS, ...stored?.sessions },
  };
}

// Outside Tauri (designing in a browser) the preferences live in memory.
let memory: StoredPrefs = {};
const memoryListeners = new Set<(p: Prefs) => void>();

function mergeInto(target: Record<string, unknown>, patch: Record<string, unknown>) {
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) delete target[key];
    else if (isObject(value) && isObject(target[key])) mergeInto(target[key] as Record<string, unknown>, value);
    else target[key] = isObject(value) ? structuredClone(value) : value;
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
