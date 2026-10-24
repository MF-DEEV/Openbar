// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's sections (NotchModule in NotchSupport.swift), in their default
// order, with the icon, shortcut letter and page traits each one has.

import {
  Bell,
  Calendar,
  CircleArrowDown,
  ClipboardList,
  Eye,
  Gauge,
  GalleryVerticalEnd,
  Inbox,
  type LucideIcon,
  Music,
  NotebookText,
  ScanLine,
  SlidersHorizontal,
  Sparkles,
  Timer,
  Volume2,
  WandSparkles,
  Webcam,
} from "lucide-react";

export type ModuleId =
  | "controls"
  | "mixer"
  | "music"
  | "clipboard"
  | "captures"
  | "files"
  | "system"
  | "tools"
  | "calendar"
  | "notifications"
  | "timer"
  | "camera"
  | "downloads"
  | "scratchpad"
  | "agents"
  | "sessions"
  | "watch";

/** How a page meets the top of the island (NotchPageTop). */
export type PageTop = "row" | "wrapsCamera" | "none";

export interface ModuleInfo {
  id: ModuleId;
  icon: LucideIcon;
  /** Ctrl+Alt+key jumps to the section (⌥⌘+key on the Mac). */
  shortcutKey: string;
  /** Tile tint in the sections gallery. */
  tint?: string;
  pageTop: PageTop;
  /** A page with a width of its own narrows the island under a preset. */
  naturalWidth?: number;
}

export const MODULES: readonly ModuleInfo[] = [
  { id: "controls", icon: SlidersHorizontal, shortcutKey: "c", pageTop: "row", naturalWidth: 344 },
  { id: "mixer", icon: Volume2, shortcutKey: "v", pageTop: "row" },
  { id: "music", icon: Music, shortcutKey: "m", tint: "#ff375f", pageTop: "wrapsCamera", naturalWidth: 344 },
  { id: "clipboard", icon: ClipboardList, shortcutKey: "b", pageTop: "row" },
  { id: "captures", icon: ScanLine, shortcutKey: "s", pageTop: "row" },
  { id: "files", icon: Inbox, shortcutKey: "f", pageTop: "row" },
  { id: "system", icon: Gauge, shortcutKey: "i", pageTop: "none" },
  { id: "tools", icon: WandSparkles, shortcutKey: "t", pageTop: "row" },
  { id: "calendar", icon: Calendar, shortcutKey: "a", tint: "#ff453a", pageTop: "row" },
  { id: "notifications", icon: Bell, shortcutKey: "n", pageTop: "row" },
  { id: "timer", icon: Timer, shortcutKey: "r", tint: "#ff9f0a", pageTop: "row", naturalWidth: 344 },
  { id: "camera", icon: Webcam, shortcutKey: "w", pageTop: "row" },
  { id: "downloads", icon: CircleArrowDown, shortcutKey: "d", pageTop: "row" },
  { id: "scratchpad", icon: NotebookText, shortcutKey: "p", pageTop: "row" },
  { id: "agents", icon: Sparkles, shortcutKey: "g", pageTop: "row" },
  { id: "sessions", icon: GalleryVerticalEnd, shortcutKey: "l", pageTop: "row" },
  { id: "watch", icon: Eye, shortcutKey: "o", pageTop: "row" },
];

export const moduleInfo = (id: ModuleId): ModuleInfo => MODULES.find((m) => m.id === id)!;

/** Stored order first, then any section the order does not name, minus hidden ones. */
export function visibleModules(order: ModuleId[], hidden: ModuleId[], enabled: (id: ModuleId) => boolean): ModuleId[] {
  const known = new Set(MODULES.map((m) => m.id));
  const ordered = [...new Set(order)].filter((id) => known.has(id));
  const rest = MODULES.map((m) => m.id).filter((id) => !ordered.includes(id));
  return [...ordered, ...rest].filter((id) => !hidden.includes(id) && enabled(id));
}
