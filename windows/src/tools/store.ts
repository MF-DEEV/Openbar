// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Tools page's state and what its items do (QuickLauncherService.run,
// QuickLauncherService.swift:271-330; QuickTogglesService).

import { create } from "zustand";
import type { IslandSettings } from "../island/settings";
import { modulesOf, useIsland } from "../island/store";
import { setPrefs } from "../shared/prefs";
import { bridge } from "../shared/bridge";
import { runCapture } from "./capture";
import { toggleRecording } from "./recording";
import { externals } from "./externals";
import { mergedOrder, orderedIds, splitTools, toggleOrder, withHidden } from "./model";
import { MAC_ONLY_TOOLS, type RunState, TOOL_IDS, type ToggleId, type ToggleState, type ToolId } from "./types";

export interface ToolsState {
  editing: boolean;
  /** The tool whose options card is open in edit mode. */
  optionsItem: ToolId | null;
  /** A utility hosted in place of the grid. */
  utility: "toggles" | null;
  /** Hover's or the keyboard's tile; the rail scrolls to the keyboard's. */
  selectedIndex: number;
  keyboardIndex: number | null;
  keepAwake: boolean;
  micMuted: boolean;
  toggleInfo: ToggleState | null;
  toggleRuns: Partial<Record<ToggleId, RunState>>;
  /** The Recycle Bin's confirmation sheet is up. */
  confirmingTrash: boolean;
}

export const useTools = create<ToolsState>(() => ({
  editing: false,
  optionsItem: null,
  utility: null,
  selectedIndex: 0,
  keyboardIndex: null,
  keepAwake: false,
  micMuted: false,
  toggleInfo: null,
  toggleRuns: {},
  confirmingTrash: false,
}));

/** A tool shows while Windows has it and its section, if any, is on. */
export function toolAvailable(id: ToolId, settings: IslandSettings): boolean {
  if (MAC_ONLY_TOOLS.includes(id)) return false;
  if (id === "keepAwake") return !!externals().keepAwake;
  if (id === "micMute") return !!externals().mic;
  if (id === "clipboard" || id === "scratchpad") return modulesOf({ settings }).includes(id);
  return true;
}

export function toolLists(settings: IslandSettings) {
  return splitTools(settings.toolsOrder, TOOL_IDS, settings.toolsHidden, (id) => toolAvailable(id, settings));
}

export function setToolHidden(id: ToolId, hide: boolean) {
  const { toolsHidden } = useIsland.getState().settings;
  if (hide && useTools.getState().optionsItem === id) useTools.setState({ optionsItem: null });
  void setPrefs({ island: { toolsHidden: withHidden(toolsHidden, id, hide) } });
}

/** The grid's visible tools in their new order. */
export function reorderTools(visible: ToolId[]) {
  const { toolsOrder } = useIsland.getState().settings;
  void setPrefs({ island: { toolsOrder: mergedOrder(orderedIds(toolsOrder, TOOL_IDS), visible) } });
}

/** Each opening starts on the grid, out of edit mode, the first tile chosen. */
export function resetTools() {
  useTools.setState({ editing: false, optionsItem: null, utility: null, selectedIndex: 0, keyboardIndex: 0 });
}

export function runTool(id: ToolId) {
  if (useTools.getState().editing) return;
  const island = useIsland.getState();
  switch (id) {
    case "keepAwake":
      void externals().keepAwake?.toggle();
      return;
    case "micMute":
      void externals().mic?.toggle();
      return;
    case "toggles":
      useTools.setState({ utility: "toggles" });
      return;
    case "clipboard":
    case "scratchpad":
      island.select(id);
      return;
    case "screenshot":
      void runCapture("screenshot");
      return;
    case "screenRecorder":
      toggleRecording();
      return;
    case "screenOCR":
      void runCapture("text");
      return;
    case "colorPicker":
      void runCapture("color");
      return;
  }
}

/** Keep awake and the microphone report their state as it changes. */
export async function subscribeExternals() {
  const { keepAwake, mic } = externals();
  if (keepAwake) {
    await keepAwake.onState((on) => useTools.setState({ keepAwake: on }));
    const now = await keepAwake.current?.();
    if (now !== undefined) useTools.setState({ keepAwake: now });
  }
  if (mic) {
    await mic.onState((muted) => useTools.setState({ micMuted: muted }));
    const now = await mic.current?.();
    if (now !== undefined) useTools.setState({ micMuted: now });
  }
}

// MARK: Quick toggles

/** The rows the list shows: switched on and possible here. */
export function visibleToggles(settings: IslandSettings, info: ToggleState | null): ToggleId[] {
  return toggleOrder(settings).filter((id) => {
    if (settings.togglesHidden.includes(id)) return false;
    if (id === "micMute") return !!externals().mic;
    if (id === "desktopIcons") return info === null || info.desktopIconsShown !== null;
    return true;
  });
}

export async function refreshToggles() {
  const info = await bridge.toggles.state();
  if (info) useTools.setState({ toggleInfo: info });
}

const failTimers: Partial<Record<ToggleId, ReturnType<typeof setTimeout>>> = {};

function setRun(id: ToggleId, state: RunState | null) {
  useTools.setState((s) => {
    const toggleRuns = { ...s.toggleRuns };
    if (state) toggleRuns[id] = state;
    else delete toggleRuns[id];
    return { toggleRuns };
  });
}

/** Success clears at once (the row is the feedback); a failure shows for 2.4 s. */
async function perform(id: ToggleId) {
  if (useTools.getState().toggleRuns[id] === "running") return;
  clearTimeout(failTimers[id]);
  setRun(id, "running");
  try {
    await bridge.toggles.run(id);
    setRun(id, null);
  } catch {
    setRun(id, "failed");
    failTimers[id] = setTimeout(() => {
      if (useTools.getState().toggleRuns[id] === "failed") setRun(id, null);
    }, 2400);
  }
  void refreshToggles();
}

/** Actions that take over the screen close the island first, then run 0.15 s later. */
const TAKES_OVER: readonly ToggleId[] = ["lockScreen", "displayOff", "screenSaver"];

export function runToggle(id: ToggleId) {
  if (id === "micMute") {
    void externals().mic?.toggle();
    return;
  }
  if (id === "emptyTrash") {
    useTools.setState({ confirmingTrash: true });
    return;
  }
  if (TAKES_OVER.includes(id)) {
    useIsland.getState().collapse();
    setTimeout(() => void perform(id), 150);
    return;
  }
  void perform(id);
}

export function confirmTrash(confirmed: boolean) {
  useTools.setState({ confirmingTrash: false });
  if (confirmed) void perform("emptyTrash");
}
