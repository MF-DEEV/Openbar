// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What New session last started with in each project, and its safe commands
// (#270; NewSessionChoices.swift and SafeCommands.swift on the Mac).

import type { NewSessionEffort, NewSessionModel, Provider } from "./types";

export type SafeKind = "build" | "test" | "lint" | "git" | "custom";

export interface SafeEntry {
  command: string;
  kind: SafeKind;
}

/** Every field optional, so a later field reads as unset in an older entry. */
export interface NewSessionChoices {
  agent?: Provider;
  /** The Claude account's id. */
  account?: string;
  /** The model key per agent. */
  models?: Partial<Record<Provider, string>>;
  /** The effort per agent and model (#289), by `effortKey`; "" is the model's default. */
  efforts?: Record<string, string>;
  worktree?: boolean;
  background?: boolean;
  safe?: boolean;
  /** Suggested commands turned off. */
  safeOff?: string[];
  /** Commands the person added, kept on until removed. */
  safeCustom?: string[];
  used?: number;
}

export const CHOICES_KEY = "openbar.newSession.byProject";
/** How many projects are kept; the least recently used go first. */
export const CHOICES_KEPT = 50;

/** Paths compare without case or trailing separators on Windows. */
export function projectKey(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return /^[a-z]:|\\/i.test(trimmed) ? trimmed.toLowerCase() : trimmed;
}

function readAll(storage: Pick<Storage, "getItem"> | undefined): Record<string, NewSessionChoices> {
  try {
    const parsed: unknown = JSON.parse(storage?.getItem(CHOICES_KEY) ?? "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, NewSessionChoices>)
      : {};
  } catch {
    return {};
  }
}

function storage(): Storage | undefined {
  try {
    return localStorage;
  } catch {
    return undefined;
  }
}

export function loadChoices(path: string, from = storage()): NewSessionChoices | null {
  return readAll(from)[projectKey(path)] ?? null;
}

export function saveChoices(path: string, choices: NewSessionChoices, now = Date.now(), to = storage()) {
  const all = readAll(to);
  all[projectKey(path)] = { ...choices, used: now };
  const keys = Object.keys(all);
  if (keys.length > CHOICES_KEPT) {
    keys
      .sort((a, b) => (all[a].used ?? 0) - (all[b].used ?? 0))
      .slice(0, keys.length - CHOICES_KEPT)
      .forEach((key) => delete all[key]);
  }
  try {
    to?.setItem(CHOICES_KEY, JSON.stringify(all));
  } catch {
    // Private storage off: the choices last this visit.
  }
}

/** The efforts all projects last started with, for a project with none of its own. */
export const EFFORTS_KEY = "openbar.newSession.efforts";

/** Where an effort is kept: the agent and the model's key ("" for the agent's default model). */
export const effortKey = (agent: Provider, model: string) => `${agent}|${model}`;

export function lastEfforts(from = storage()): Record<string, string> {
  try {
    const parsed: unknown = JSON.parse(from?.getItem(EFFORTS_KEY) ?? "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? (parsed as Record<string, string>) : {};
  } catch {
    return {};
  }
}

export function rememberEffort(key: string, effort: string, to = storage()) {
  try {
    to?.setItem(EFFORTS_KEY, JSON.stringify({ ...lastEfforts(to), [key]: effort }));
  } catch {
    // Private storage off: the choice lasts this visit.
  }
}

/** The level picked for a model, if it takes it; none is the model's default. */
export function pickedEffort(
  model: NewSessionModel | undefined,
  picked: string | undefined,
): NewSessionEffort | undefined {
  return picked ? model?.efforts?.find((e) => e.id === picked) : undefined;
}

/** The level a session starts at: the one picked, else the one the default gives when it can be told. */
export function shownEffort(model: NewSessionModel | undefined, picked: NewSessionEffort | undefined) {
  return picked ?? model?.efforts?.find((e) => e.id === model.defaultEffort);
}

/** The agents whose sessions can start with the list. */
export const offersSafeCommands = (agent: Provider) => agent === "claude" || agent === "codex";

const JOINERS = /[;&|<>`$(){}\n\r*"'\\]/;

/** A command a person may add: one line, plain words, no joiners, short. */
export function cleanedCommand(text: string): string | null {
  const command = text.split(/\s+/).filter(Boolean).join(" ");
  if (!command || command.length > 80 || JOINERS.test(command)) return null;
  return command;
}

/** The commands a session starts allowed to run: the suggested ones still
 * on (git's three only in a repository), then the person's own. */
export function safeList(
  on: boolean,
  agent: Provider,
  suggested: readonly SafeEntry[],
  off: readonly string[],
  custom: readonly string[],
  isRepository: boolean,
): string[] {
  if (!on || !offersSafeCommands(agent)) return [];
  return [
    ...shownSuggested(suggested, isRepository)
      .map((e) => e.command)
      .filter((c) => !off.includes(c)),
    ...custom,
  ];
}

export function shownSuggested(suggested: readonly SafeEntry[], isRepository: boolean): SafeEntry[] {
  return isRepository ? [...suggested] : suggested.filter((e) => e.kind !== "git");
}
