// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Settings › Behavior › Keep windows below the island (Linux): the window
// manager keeps a strip the closed island's height free at the top of the
// display, so a maximized browser's tabs start below the island instead of
// under it, as apps start below the Mac's menu bar.

import { onLinux } from "../shared/platform";
import type { Geometry } from "./geometry";
import type { IslandSettings } from "./settings";

/**
 * The strip to keep in CSS pixels, null for none: the closed island's own
 * height, and nothing while the island is off or hidden until hover, which
 * would leave an empty band.
 */
export function roomHeight(
  settings: Pick<IslandSettings, "enabled" | "makeRoom" | "opening">,
  geometry: Pick<Geometry, "stripHeight" | "floatingDrop">,
  linux = onLinux(),
): number | null {
  if (!linux || !settings.enabled || !settings.makeRoom || settings.opening === "hiddenUntilHover") return null;
  const height = geometry.stripHeight + geometry.floatingDrop;
  return Number.isFinite(height) && height > 0 ? Math.ceil(height) : null;
}
