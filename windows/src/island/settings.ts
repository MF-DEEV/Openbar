// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's own preferences (spec 01-shell §7.1), with the Mac's defaults
// except where Windows differs (§7.0): the hanging notch is the default
// outline, the global shortcut and hiding in full screen start on, and the
// Sessions and Agents sections start on.

import type { LayoutPreset, Silhouette } from "./geometry";
import type { ModuleId } from "./modules";

export type QuickSide = "left" | "right" | "bottom";
export type QuickAction =
  { kind: "explore" } | { kind: "settings" } | { kind: "pin" } | { kind: "module"; module: ModuleId };

export interface QuickButton {
  id: string;
  action: QuickAction;
  side: QuickSide;
  label?: string;
}

export type OpeningMode = "click" | "hoverPreview" | "hoverExpand" | "hiddenUntilHover";
export type DisplayChoice = "primary" | "cursor" | "all";
export type HomeDestination = ModuleId | "explore";
export type ColorPickerFormat = "hex" | "rgb" | "hsl";
export type ScratchpadRetention = "never" | "day" | "week" | "month";

export interface IslandSettings {
  enabled: boolean;
  silhouette: Silhouette;
  layout: LayoutPreset;
  customWidth: number;
  customHeight: number;
  notchWidth: number;
  notchHeight: number;
  roundness: number;
  shoulderWidth: number;
  shoulderHeight: number;
  capsuleFit: { width: number; height: number; drop: number };
  outline: boolean;
  opening: OpeningMode;
  hiddenShowsActivity: boolean;
  /** Seconds. */
  hoverDelay: number;
  closeDelay: number;
  shortcutEnabled: boolean;
  shortcut: string;
  gestures: boolean;
  returnHome: boolean;
  homeModule: HomeDestination;
  opensActivity: boolean;
  hideInFullscreen: boolean;
  display: DisplayChoice;
  showInCaptures: boolean;
  hiddenModules: ModuleId[];
  moduleOrder: ModuleId[];
  quickAccess: QuickButton[];
  /** Per-section switches the Mac keeps as notch*Enabled. */
  sections: Partial<Record<ModuleId, boolean>>;
  language: string;
  /** Music takes the closed island while it plays (notchShowPlayingMusic). */
  showPlayingMusic: boolean;
  /** A new song raises a notice (notchTrackChange). */
  trackChange: boolean;
  downloadProgress: boolean;
  microphoneStatus: boolean;
  downloadFolders: string[];
  /** Tools: the grid's order and hidden tools (quickLauncherItemOrder / quickLauncherHiddenItems). */
  toolsOrder: string[];
  toolsHidden: string[];
  /** The colour picker copies in this format (colorPickerFormat); bare hex drops the # (colorPickerBareHex). */
  colorPickerFormat: ColorPickerFormat;
  colorPickerBareHex: boolean;
  /** Quick toggles: row order (panelToggleOrder) and the rows switched off (panelToggle*). */
  togglesOrder: string[];
  togglesHidden: string[];
  /** Volumes "Eject all disks" leaves alone, by label or drive (diskEjectExcludedVolumes). */
  ejectExcluded: string[];
  /** Scratchpad text size, 10–22 (scratchpadTextSize), and when unused text clears (scratchpadRetention). */
  scratchpadTextSize: number;
  scratchpadRetention: ScratchpadRetention;
}

export const DEFAULT_QUICK_ACCESS: QuickButton[] = [
  { id: "00000000-0000-4000-8000-000000000001", action: { kind: "explore" }, side: "left" },
  { id: "00000000-0000-4000-8000-000000000002", action: { kind: "module", module: "timer" }, side: "left" },
  { id: "00000000-0000-4000-8000-000000000003", action: { kind: "settings" }, side: "right" },
  { id: "00000000-0000-4000-8000-000000000004", action: { kind: "module", module: "mixer" }, side: "right" },
  { id: "00000000-0000-4000-8000-000000000005", action: { kind: "module", module: "music" }, side: "bottom" },
];

export const DEFAULT_SETTINGS: IslandSettings = {
  enabled: true,
  silhouette: "notch",
  layout: "spacious",
  customWidth: 440,
  customHeight: 480,
  notchWidth: 135,
  notchHeight: 24,
  roundness: 100,
  shoulderWidth: 100,
  shoulderHeight: 100,
  capsuleFit: { width: 0, height: 0, drop: 0 },
  outline: false,
  opening: "click",
  hiddenShowsActivity: false,
  hoverDelay: 0.25,
  closeDelay: 0.18,
  shortcutEnabled: true,
  shortcut: "Ctrl+Alt+Space",
  gestures: true,
  returnHome: false,
  homeModule: "controls",
  opensActivity: true,
  hideInFullscreen: true,
  display: "primary",
  showInCaptures: true,
  hiddenModules: [],
  moduleOrder: [],
  quickAccess: DEFAULT_QUICK_ACCESS,
  sections: { agents: true, sessions: true },
  language: "auto",
  showPlayingMusic: true,
  trackChange: true,
  downloadProgress: true,
  microphoneStatus: true,
  downloadFolders: [],
  toolsOrder: [],
  toolsHidden: [],
  colorPickerFormat: "hex",
  colorPickerBareHex: false,
  togglesOrder: [],
  togglesHidden: [],
  ejectExcluded: [],
  scratchpadTextSize: 13,
  scratchpadRetention: "never",
};

/** Sections the Mac switches off by default; every other section starts on. */
const OFF_BY_DEFAULT: ModuleId[] = [];

export function sectionEnabled(settings: IslandSettings, id: ModuleId): boolean {
  return settings.sections[id] ?? !OFF_BY_DEFAULT.includes(id);
}

export const opensOnHover = (s: IslandSettings) => s.opening !== "click";
export const hoverExpands = (s: IslandSettings) => s.opening === "hoverExpand" || s.opening === "hiddenUntilHover";
