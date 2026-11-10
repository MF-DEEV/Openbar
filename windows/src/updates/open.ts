// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { openUrl } from "@tauri-apps/plugin-opener";
import { inTauri } from "../shared/bridge";
import type { Release } from "./releases";

/** The release's page in the browser, where its installer and notes are. */
export function openRelease(release: Release) {
  if (inTauri) void openUrl(release.url);
  else window.open(release.url);
}
