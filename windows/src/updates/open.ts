// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { inTauri } from "../shared/bridge";
import { onLinux } from "../shared/platform";
import type { Release } from "./releases";

/** The release's page in the browser, where its installer and notes are. */
export function openRelease(release: Release) {
  if (inTauri) void openUrl(release.url);
  else window.open(release.url);
}

/** The update prompt window on Windows; the release page where the app can't update itself. */
export function openPrompt(release: Release) {
  if (inTauri && !onLinux()) void invoke("update_window_open");
  else openRelease(release);
}
