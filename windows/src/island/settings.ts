// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's own preferences (spec 01-shell §7.1), with the Mac's defaults
// except where Windows differs (§7.0): the hanging notch is the default
// outline, the global shortcut and hiding in full screen start on, and the
// Sessions and Agents sections start on.

import type { LayoutPreset, Silhouette } from "./geometry";
import type { ModuleId } from "./modules";
import type { MonitorMetric, NetworkSpeedUnit } from "../system/types";
import { DEFAULT_HIDDEN_CONTROLS, type ControlItem } from "../controls/types";
import type { AgentCard, AgentPeriod, AgentReadout, CodexProfile, LimitDisplay, LimitFocus } from "../agents/types";
import type { WatchCondition } from "../watch/types";
import type { MascotFrequency, MascotPalette, MascotShape, MascotSide, MascotStyle } from "../mascot/model";

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
export type IdleContent = "none" | "battery" | "music" | "agents";
export type DisplayChoice = "primary" | "cursor" | "all";
export type HomeDestination = ModuleId | "explore";
export type ColorPickerFormat = "hex" | "rgb" | "hsl";
export type ScratchpadRetention = "never" | "day" | "week" | "month";

export interface IslandSettings {
  /** Camera device chosen in the live preview; empty follows the system default. */
  cameraDeviceId: string;
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
  /** The capture tools' controls at the top of the selection (notchCaptureControls). */
  captureControls: boolean;
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
  /** Idle player to open; empty chooses the last music app, then an installed player. */
  preferredPlayer: string;
  /** Automatic selection may follow browsers and other non-music media sessions. */
  includeOtherPlayers: boolean;
  /** What rests beside the camera (macOS notchIdleContent). */
  idleContent: IdleContent;
  lowBatteryTint: boolean;
  lowBatteryThreshold: number;
  lowBatteryEarly: boolean;
  lowBatteryEarlyThreshold: number;
  /** Also on the tray icon, the Mac's menu bar reading. */
  lowBatteryTray: boolean;
  /** Ask LRCLIB for lyrics while they are open (macOS notchLyricsOnline, off by default). */
  lyricsOnline: boolean;
  /** A new song raises a notice (notchTrackChange). */
  trackChange: boolean;
  downloadProgress: boolean;
  microphoneStatus: boolean;
  downloadFolders: string[];
  /** Timer (spec 04 §4.5): the mode set up next and its values; a running cycle keeps its own copy. */
  timerMode: "timer" | "pomodoro" | "stopwatch";
  /** 1…180. */
  timerMinutes: number;
  timerSoundEnabled: boolean;
  /** Hides a running countdown from the closed island; a finished timer always shows. */
  hideTimerCountdown: boolean;
  pomodoroFocusMinutes: number;
  pomodoroShortBreakMinutes: number;
  pomodoroLongBreakMinutes: number;
  pomodoroLongBreakInterval: number;
  pomodoroTotalSessions: number;
  /** Seconds between System readings: 1, 2 or 5 (monitorInterval). */
  monitorInterval: number;
  networkSpeedUnit: NetworkSpeedUnit;
  /** System cards switched off (the Mac's monitor* features). */
  hiddenSystemMetrics: MonitorMetric[];
  /** Every timed event counts down in the closed island (notchCalendarCountdown). */
  calendarCountdown: boolean;
  /** The event under way counts down to its end (notchCalendarTimeLeft). */
  calendarTimeLeft: boolean;
  calendarWeekNumbers: boolean;
  /** Calendar ids left out; a calendar added later starts shown (notchCalendarExcluded). */
  calendarExcluded: string[];
  /** Events chosen to count down one by one: countdown key → end, epoch ms (notchCalendarChosenCountdowns). */
  calendarChosenCountdowns: Record<string, number>;
  /** The home page's order and hidden items (notchControlOrder, notchHiddenControls). */
  controlOrder: ControlItem[];
  hiddenControls: ControlItem[];
  /** The brightness level instead of its setup row (brightnessControlEnabled). */
  brightnessControlEnabled: boolean;
  /** A running Keep Awake session takes the closed island (notchKeepAwakeActivity). */
  keepAwakeActivity: boolean;
  /** Minutes the Keep Awake tile runs for; 0 until turned off (defaultDurationMinutes). */
  keepAwakeDuration: number;
  /** The volume notice when the level changes outside the island (notchVolume). */
  volumeNotice: boolean;
  /** Pinned apps and the mixer's order, by exe path (mixerAppArrangement). */
  mixerAppArrangement: { order: string[]; pinned: string[] };
  /** Apps kept out of the mixer: exe path → name (mixerHiddenApps). */
  mixerHiddenApps: Record<string, string>;
  mixerHideInactiveApps: boolean;
  /** Lower the speakers when headphones disconnect, to this 0–100 level (mixerLowerVolumeOnHeadphonesDisconnect, mixerHeadphonesDisconnectVolumePercent). */
  mixerLowerVolumeOnHeadphonesDisconnect: boolean;
  mixerHeadphonesDisconnectVolumePercent: number;
  /** The volume keys move 1% at a time (preciseVolumeRollerEnabled). */
  preciseVolumeRollerEnabled: boolean;
  /** One shortcut moves the output through the chosen devices (soundOutputSwitcher*). The shortcut is an accelerator such as "Ctrl+Alt+Shift+KeyS". */
  soundOutputSwitcherEnabled: boolean;
  soundOutputSwitcherShortcut: string;
  soundOutputSwitcherDevices: string[];
  /** Tools: the grid's order and hidden tools (quickLauncherItemOrder / quickLauncherHiddenItems). */
  toolsOrder: string[];
  toolsHidden: string[];
  /** The colour picker copies in this format (colorPickerFormat); bare hex drops the # (colorPickerBareHex). */
  colorPickerFormat: ColorPickerFormat;
  colorPickerBareHex: boolean;
  /** Screenshot preview lifetime in seconds; zero keeps it open. */
  screenshotPreviewDuration: number;
  screenshotPreviewDismissOnBlur: boolean;
  /** Quick toggles: row order (panelToggleOrder) and the rows switched off (panelToggle*). */
  togglesOrder: string[];
  togglesHidden: string[];
  /** Volumes "Eject all disks" leaves alone, by label or drive (diskEjectExcludedVolumes). */
  ejectExcluded: string[];
  /** Scratchpad text size, 10–22 (scratchpadTextSize), and when unused text clears (scratchpadRetention). */
  scratchpadTextSize: number;
  scratchpadRetention: ScratchpadRetention;
  /** Clipboard history (clipboardHistory*; spec 05 §3.2). Off until the person turns it on. */
  clipboardHistoryEnabled: boolean;
  /** One of CLIPBOARD_LIMITS; 0 is unlimited. */
  clipboardHistoryLimit: number;
  /** One of CLIPBOARD_RETENTION_DAYS; 0 keeps entries. */
  clipboardHistoryRetentionDays: number;
  clipboardHistoryIncludeImagesFiles: boolean;
  clipboardFilterSensitiveText: boolean;
  /** Exe names whose copies are never saved ("KeePass.exe"). */
  clipboardIgnoredApps: string[];
  /** notchClipboardCardSize. */
  clipboardCardSize: "compact" | "comfortable";
  /** The "Copied" notice (notchClipboard). */
  clipboardNotice: boolean;
  /** Files: a drag over the closed island shows where to drop (notchDragReveal). */
  dragReveal: boolean;
  /** Files: tiles another app accepted leave the shelf; pinned ones stay. */
  shelfRemoveAfterDrop: boolean;
  /** Files: the island closes once another app accepts dragged tiles. */
  shelfCloseAfterDrop: boolean;
  // AI Agents (spec 03 §11; the Mac's notchAgents* keys).
  /** Read each agent's logs; at least one stays on. */
  agentsClaude: boolean;
  agentsCodex: boolean;
  agentsOpenCode: boolean;
  agentsCopilot: boolean;
  /** The cards' order; cards it leaves out follow in their default order. */
  agentsCardOrder: AgentCard[];
  agentsHiddenCards: AgentCard[];
  /** The period Spending, Trend, Models and Projects cover. */
  agentsPeriod: AgentPeriod;
  agentsLimitDisplay: LimitDisplay;
  agentsLimitFocus: LimitFocus;
  /** The closed island shows an agent at work. */
  agentsLiveActivity: boolean;
  agentsReadout: AgentReadout;
  agentsFinishAlert: boolean;
  /** Seconds: the shortest turn worth a notice. */
  agentsFinishMinimum: number;
  agentsLimitAlert: boolean;
  /** Percent used that earns a warning. */
  agentsLimitThreshold: number;
  /** USD of API value a day; 0 is off. */
  agentsDailyBudget: number;
  agentsPriceUpdates: boolean;
  /** Codex homes with limits cards of their own (notchAgentsCodexProfiles). */
  agentsCodexProfiles: CodexProfile[];
  /** Watch (spec 04 §6.3): a chime when a watch ends (notchWatchSound). */
  watchSound: boolean;
  /** The rule a watch starts with (notchWatchCondition); typed text and numbers aren't kept. */
  watchCondition: WatchCondition;
  // The companion (spec 05 §12.1; the Mac's notchMascot* keys).
  mascotEnabled: boolean;
  /** Passes through the island now and then. */
  mascotVisits: boolean;
  /** Comes out to react to what the island sees. */
  mascotReactions: boolean;
  mascotStyle: MascotStyle;
  /** The minimal style's body. */
  mascotShape: MascotShape;
  mascotPalette: MascotPalette;
  /** Beside the notch; a capsule holds it in the middle. */
  mascotSide: MascotSide;
  mascotVisitFrequency: MascotFrequency;
  /** Hops into the island after a quiet while. */
  mascotHidesWhenIdle: boolean;
}

export const CLIPBOARD_LIMITS = [20, 50, 100, 250, 500, 1000, 10000, 0];
export const CLIPBOARD_RETENTION_DAYS = [1, 7, 30, 90, 365, 0];

export const DEFAULT_QUICK_ACCESS: QuickButton[] = [
  { id: "00000000-0000-4000-8000-000000000001", action: { kind: "explore" }, side: "left" },
  { id: "00000000-0000-4000-8000-000000000002", action: { kind: "module", module: "timer" }, side: "left" },
  { id: "00000000-0000-4000-8000-000000000003", action: { kind: "settings" }, side: "right" },
  { id: "00000000-0000-4000-8000-000000000004", action: { kind: "module", module: "mixer" }, side: "right" },
  { id: "00000000-0000-4000-8000-000000000005", action: { kind: "module", module: "music" }, side: "bottom" },
];

/** The Mac's Control-Option-Command-S on Windows' three modifiers (audio/switcher.rs DEFAULT_SHORTCUT). */
export const DEFAULT_SWITCHER_SHORTCUT = "Ctrl+Alt+Shift+KeyS";

export const DEFAULT_SETTINGS: IslandSettings = {
  cameraDeviceId: "",
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
  captureControls: true,
  returnHome: true,
  homeModule: "sessions",
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
  preferredPlayer: "",
  includeOtherPlayers: true,
  idleContent: "music",
  lowBatteryTint: false,
  lowBatteryThreshold: 10,
  lowBatteryEarly: false,
  lowBatteryEarlyThreshold: 20,
  lowBatteryTray: true,
  lyricsOnline: false,
  trackChange: true,
  downloadProgress: true,
  microphoneStatus: true,
  downloadFolders: [],
  timerMode: "timer",
  timerMinutes: 15,
  timerSoundEnabled: true,
  hideTimerCountdown: false,
  pomodoroFocusMinutes: 25,
  pomodoroShortBreakMinutes: 5,
  pomodoroLongBreakMinutes: 15,
  pomodoroLongBreakInterval: 4,
  pomodoroTotalSessions: 4,
  monitorInterval: 2,
  networkSpeedUnit: "bytes",
  hiddenSystemMetrics: [],
  calendarCountdown: false,
  calendarTimeLeft: false,
  calendarWeekNumbers: false,
  calendarExcluded: [],
  calendarChosenCountdowns: {},
  controlOrder: [],
  hiddenControls: DEFAULT_HIDDEN_CONTROLS,
  brightnessControlEnabled: false,
  keepAwakeActivity: false,
  keepAwakeDuration: 0,
  volumeNotice: true,
  mixerAppArrangement: { order: [], pinned: [] },
  mixerHiddenApps: {},
  mixerHideInactiveApps: false,
  mixerLowerVolumeOnHeadphonesDisconnect: false,
  mixerHeadphonesDisconnectVolumePercent: 25,
  preciseVolumeRollerEnabled: false,
  soundOutputSwitcherEnabled: false,
  soundOutputSwitcherShortcut: DEFAULT_SWITCHER_SHORTCUT,
  soundOutputSwitcherDevices: [],
  toolsOrder: [],
  toolsHidden: [],
  colorPickerFormat: "hex",
  colorPickerBareHex: false,
  screenshotPreviewDuration: 8,
  screenshotPreviewDismissOnBlur: false,
  togglesOrder: [],
  togglesHidden: [],
  ejectExcluded: [],
  scratchpadTextSize: 13,
  scratchpadRetention: "never",
  clipboardHistoryEnabled: false,
  clipboardHistoryLimit: 50,
  clipboardHistoryRetentionDays: 0,
  clipboardHistoryIncludeImagesFiles: true,
  clipboardFilterSensitiveText: false,
  clipboardIgnoredApps: [],
  clipboardCardSize: "comfortable",
  clipboardNotice: true,
  dragReveal: true,
  shelfRemoveAfterDrop: true,
  shelfCloseAfterDrop: false,
  agentsClaude: true,
  agentsCodex: true,
  agentsOpenCode: true,
  agentsCopilot: true,
  agentsCardOrder: [],
  agentsHiddenCards: [],
  agentsPeriod: "today",
  agentsLimitDisplay: "remaining",
  agentsLimitFocus: "mostUsed",
  agentsLiveActivity: true,
  agentsReadout: "elapsed",
  agentsFinishAlert: true,
  agentsFinishMinimum: 60,
  agentsLimitAlert: true,
  agentsLimitThreshold: 80,
  agentsDailyBudget: 0,
  agentsPriceUpdates: true,
  agentsCodexProfiles: [],
  watchSound: true,
  watchCondition: "changes",
  mascotEnabled: false,
  mascotVisits: true,
  mascotReactions: true,
  mascotStyle: "minimal",
  mascotShape: "ball",
  mascotPalette: "pearl",
  mascotSide: "left",
  mascotVisitFrequency: "normal",
  mascotHidesWhenIdle: true,
};

/** Sections the Mac switches off by default; every other section starts on. */
const OFF_BY_DEFAULT: ModuleId[] = [];

export function sectionEnabled(settings: IslandSettings, id: ModuleId): boolean {
  return settings.sections[id] ?? !OFF_BY_DEFAULT.includes(id);
}

export const opensOnHover = (s: IslandSettings) => s.opening !== "click";
export const hoverExpands = (s: IslandSettings) => s.opening === "hoverExpand" || s.opening === "hiddenUntilHover";
