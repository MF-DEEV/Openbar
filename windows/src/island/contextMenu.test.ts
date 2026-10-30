// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { islandMenuEntries } from "./contextMenu";
import { useIsland } from "./store";
import { MODULES } from "./modules";

const initial = useIsland.getState();
beforeEach(() => {
  vi.useFakeTimers();
  useIsland.getState().collapse();
  useIsland.setState(initial, true);
});
afterEach(() => {
  useIsland.getState().collapse();
  vi.useRealTimers();
  vi.restoreAllMocks();
  useIsland.setState(initial, true);
});

function select(label: string) {
  const item = islandMenuEntries().find((item) => "label" in item && item.label === label);
  if (!item || item.kind === "separator" || item.kind === "header") throw new Error(`Missing menu item: ${label}`);
  item.onSelect?.();
}

it("uses the Mac's Sections gallery and Back action, omitting hidden sections", () => {
  useIsland.setState({ settings: { ...initial.settings, hiddenModules: ["music"] } });
  select("Sections");
  expect(useIsland.getState()).toMatchObject({ expanded: true, showingSections: true });
  select("Back");
  expect(useIsland.getState()).toMatchObject({ expanded: true, showingSections: false });
  useIsland.setState({ settings: { ...initial.settings, hiddenModules: MODULES.map((module) => module.id) } });
  expect(islandMenuEntries().some((item) => "label" in item && item.label === "Sections")).toBe(false);
});

it("reads pin state at action time and reflects Pin/Unpin", () => {
  const entry = islandMenuEntries().find((item) => "label" in item && item.label === "Pin");
  useIsland.getState().setPinned(true);
  if (entry && entry.kind !== "separator" && entry.kind !== "header") entry.onSelect?.();
  expect(useIsland.getState().pinned).toBe(false);
  select("Pin");
  expect(useIsland.getState().pinned).toBe(true);
  select("Unpin");
  expect(useIsland.getState().pinned).toBe(false);
});

it("opens real settings and collapses even a pinned island", () => {
  const settings = vi.spyOn(bridge, "openSettings").mockResolvedValue(undefined);
  useIsland.getState().open("music");
  useIsland.getState().setPinned(true);
  select("Settings");
  expect(settings).toHaveBeenCalledOnce();
  expect(useIsland.getState()).toMatchObject({ expanded: false, pinned: false });
  useIsland.getState().open("controls");
  useIsland.getState().setPinned(true);
  select("Collapse");
  expect(useIsland.getState()).toMatchObject({ expanded: false, pinned: false });
});

it("holds the current surface while a menu cancels pending hover and exit timers", () => {
  useIsland.setState({ settings: { ...initial.settings, opening: "hoverExpand" } });
  useIsland.getState().pointerEnter();
  useIsland.getState().setHeld(true);
  vi.runAllTimers();
  expect(useIsland.getState().expanded).toBe(false);
  useIsland.getState().setHeld(false);
  useIsland.getState().open("music", { byHover: true });
  useIsland.getState().pointerExit();
  useIsland.getState().setHeld(true);
  vi.runAllTimers();
  useIsland.getState().pointerEnter();
  useIsland.getState().pointerExit();
  vi.runAllTimers();
  expect(useIsland.getState()).toMatchObject({ expanded: true, selected: "music" });
});
