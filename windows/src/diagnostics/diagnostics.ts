// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Export diagnostics (#230): the parts of the file, in the order the screen
// before saving lists them, and what the settings window adds to it. The
// parts are the backend's (src-tauri/src/diagnostics/report.rs) and the
// Mac app's, so the screen shows what the file holds.

import { MODULES } from "../island/modules";
import { sectionEnabled } from "../island/settings";
import type { Prefs } from "../shared/prefs";

export type PartId = "system" | "features" | "agents" | "events" | "hosts" | "logs";

export interface Part {
  id: PartId;
  /** Left out when switched off; the others are always in. */
  optional: boolean;
}

export const PARTS: readonly Part[] = [
  { id: "system", optional: false },
  { id: "features", optional: false },
  { id: "agents", optional: false },
  { id: "events", optional: true },
  { id: "hosts", optional: true },
  { id: "logs", optional: true },
];

/** How many events and days the file keeps (journal::CAPACITY, log::EXPORT_DAYS). */
export const EVENT_COUNT = 200;
export const LOG_DAYS = 3;

export interface Feature {
  id: string;
  group: string;
  on: boolean;
}

/** Which features are on, as this window's preferences say. */
export function features(prefs: Prefs): Feature[] {
  const island = prefs.island;
  const list: Feature[] = [{ id: "island", group: "island", on: island.enabled }];
  for (const module of MODULES) {
    list.push({
      id: module.id,
      group: "island",
      on: island.enabled && !island.hiddenModules.includes(module.id) && sectionEnabled(island, module.id),
    });
  }
  for (const [id, value] of Object.entries(island)) {
    if (typeof value === "boolean" && id !== "enabled") list.push({ id, group: "settings", on: value });
  }
  for (const [id, value] of Object.entries(prefs.sessions)) {
    if (typeof value === "boolean") list.push({ id, group: "sessions", on: value });
  }
  return list;
}

export interface ExportRequest {
  parts: PartId[];
  features: Feature[];
  language: string;
  channel: string;
  path?: string;
}

/** What the backend is asked for: the optional parts switched on, and what only this window knows. */
export function request(chosen: ReadonlySet<PartId>, prefs: Prefs, language: string): ExportRequest {
  return {
    parts: PARTS.filter((p) => p.optional && chosen.has(p.id)).map((p) => p.id),
    features: features(prefs),
    language,
    channel: prefs.island.betaUpdates ? "beta" : "stable",
  };
}

export const REPOSITORY = "https://github.com/MF-DEEV/Openbar";

/**
 * The GitHub issue form for a bug, with the version and system filled in.
 * The form can't take a file from an address: it says which one to drag in.
 */
export function issueUrl(
  kind: "bug" | "feature",
  details: { version: string; system: string; linux: boolean; attachment?: string },
): string {
  const url = new URL(`${REPOSITORY}/issues/new`);
  url.searchParams.set("template", kind === "bug" ? "bug_report.yml" : "feature_request.yml");
  if (kind === "bug") {
    url.searchParams.set("version", `${details.linux ? "linux" : "windows"}-v${details.version}`);
    if (details.system) url.searchParams.set("os_version", details.system);
    if (!details.linux) url.searchParams.set("platform", "Windows");
    if (details.attachment) url.searchParams.set("diagnostics", `Drag ${details.attachment} here to attach it.`);
  }
  return url.toString();
}

/** The file's name, from the path the backend saved it to. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
