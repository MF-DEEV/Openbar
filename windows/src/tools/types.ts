// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Tools section's items (QuickLauncherItem, QuickLauncherService.swift:10-43)
// and the quick toggles' actions (QuickToggleAction, QuickTogglesService.swift:8-19).

/** Every Mac tool, in the Mac's default order; raw values are the stored ids. */
export const TOOL_IDS = [
  "keepAwake",
  "cleaner",
  "toggles",
  "micMute",
  "screenOCR",
  "colorPicker",
  "clipboard",
  "windowLayout",
  "cleaning",
  "homebrew",
  "media",
  "urlCleaner",
  "uninstaller",
  "screenshot",
  "screenRecorder",
  "cameraPreview",
  "scratchpad",
] as const;
export type ToolId = (typeof TOOL_IDS)[number];

/** Mac tools with no Windows counterpart (spec 05 §7.2): never shown, not even to add back. */
export const MAC_ONLY_TOOLS: readonly ToolId[] = [
  "cleaner",
  "cleaning",
  "windowLayout",
  "homebrew",
  "media",
  "urlCleaner",
  "uninstaller",
  "cameraPreview",
];

/** Tools with an options card in edit mode. Keep awake, mic and clipboard bring theirs with their sections. */
export const TOOLS_WITH_OPTIONS: readonly ToolId[] = ["colorPicker"];

/** The Windows quick toggles in the Mac's order; keyboardLight has no Windows API. */
export const TOGGLE_IDS = [
  "darkMode",
  "micMute",
  "emptyTrash",
  "ejectDisks",
  "hiddenFiles",
  "desktopIcons",
  "lockScreen",
  "displayOff",
  "screenSaver",
] as const;
export type ToggleId = (typeof TOGGLE_IDS)[number];

/** What a toggle's row reads when it opens (toggles.rs State). */
export interface ToggleState {
  dark: boolean | null;
  hiddenFilesShown: boolean | null;
  /** Null when the desktop's icon view can't be found: the row hides. */
  desktopIconsShown: boolean | null;
  ejectable: number;
  recycleItems: number | null;
}

/** A row's short-lived state (QuickTogglesService.RunState). */
export type RunState = "running" | "failed";

export type CapturePurpose = "screenshot" | "recording" | "text" | "color";

/** What a capture tool did (capture/mod.rs Outcome). */
export type CaptureOutcome =
  | { kind: "cancelled" }
  | {
      kind: "screenshot";
      path: string | null;
      folder: string | null;
      copied: boolean;
      thumbnail: string | null;
      preview?: boolean;
      previewError?: string | null;
    }
  | { kind: "text"; text: string }
  | { kind: "noText" }
  | { kind: "noLanguage" }
  | { kind: "color"; value: string; hex: string }
  | { kind: "recording" }
  | { kind: "failed"; message: string };
