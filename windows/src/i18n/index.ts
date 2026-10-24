// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Strings for the island and settings. English lives here as the fallback;
// the other languages are generated from the Mac app's Swift string tables
// into locales/*.json (see windows/docs/spec/06-i18n.md).

const en = {
  "module.controls": "Controls",
  "module.mixer": "Mixer",
  "module.music": "Now Playing",
  "module.clipboard": "Clipboard",
  "module.captures": "Recent Captures",
  "module.files": "Files",
  "module.system": "System",
  "module.tools": "Tools",
  "module.calendar": "Calendar",
  "module.notifications": "Notifications",
  "module.timer": "Timer",
  "module.camera": "Camera",
  "module.downloads": "Downloads",
  "module.scratchpad": "Scratchpad",
  "module.agents": "AI Agents",
  "module.sessions": "Sessions",
  "module.watch": "Watch",
  "island.sections": "Sections",
  "island.settings": "Settings",
  "island.pin": "Pin",
  "island.unpin": "Unpin",
  "island.back": "Back",
  "island.open": "Open",
  "island.collapse": "Collapse",
  "island.search": "Search",
  "island.comingSoon": "Coming to Windows soon",
} as const;

export type StringKey = keyof typeof en;

let table: Partial<Record<StringKey, string>> = {};

export function setLocaleTable(next: Partial<Record<StringKey, string>>) {
  table = next;
}

export function t(key: StringKey): string {
  return table[key] ?? en[key];
}
