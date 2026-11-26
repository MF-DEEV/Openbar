// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The usage stats count the person's visits to the island's sections (#333);
// the backend keeps the counts (src-tauri/src/usage_stats) and ignores them
// while Share anonymous usage stats is off.

import { invoke } from "@tauri-apps/api/core";
import type { ModuleId } from "./modules";

// Checked here rather than taken from shared/bridge, which tests replace whole.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function noteSectionOpened(id: ModuleId): void {
  if (inTauri) void invoke("stats_section_opened", { section: id }).catch(() => {});
}
