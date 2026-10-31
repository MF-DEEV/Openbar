// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The tools one selection can switch between (ScreenCaptureTool), each with
// its number key. The number keeps its Mac meaning even where a tool is absent.

import type { CaptureTool } from "./types";

export const TOOLS: readonly { id: CaptureTool; key: string }[] = [
  { id: "screenshot", key: "1" },
  { id: "text", key: "3" },
  { id: "color", key: "4" },
];

export function toolForKey(key: string): CaptureTool | null {
  return TOOLS.find((tool) => tool.key === key)?.id ?? null;
}

/** Watch picks an area for itself; it has no tools to switch to. */
export const switchesTools = (purpose: string) => purpose !== "watch";

/** R repeats an area, so only for the tools that take one, and only once one was taken here. */
export const offersRepeat = (tool: string, lastArea: unknown) => tool !== "color" && lastArea != null;
