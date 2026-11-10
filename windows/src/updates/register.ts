// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The island's side of update checks: one a little after it starts, then every
// few hours, and a notice the first time a version is found (a tap opens its
// release page). Turned off in Settings › General.

import { Download } from "lucide-react";
import { t } from "../i18n";
import { inTauri } from "../shared/bridge";
import { useIsland } from "../island/store";
import { openRelease } from "./open";
import { useUpdates } from "./store";

const FIRST = 20_000;
const EVERY = 6 * 60 * 60_000;
const SEEN = "openbar.update.noticed";

function noticed(): string | null {
  try {
    return localStorage.getItem(SEEN);
  } catch {
    return null;
  }
}

function remember(version: string) {
  try {
    localStorage.setItem(SEEN, version);
  } catch {
    // The notice may come again after a restart; that is all.
  }
}

/** One automatic check; the Settings button checks on its own and raises no notice. */
export async function automaticCheck() {
  const island = useIsland.getState();
  if (!island.settings.enabled || !island.settings.checkUpdates) return;
  if ((await useUpdates.getState().check()) !== "available") return;
  const latest = useUpdates.getState().latest;
  if (!latest || noticed() === latest.version) return;
  remember(latest.version);
  useIsland.getState().showNotice({
    id: `update:${latest.version}`,
    kind: "update",
    title: t("s.updateNotifyTitle"),
    detail: `${t("s.updateAvailablePrefix")} ${latest.version}`,
    priority: 1,
    duration: 8,
    icon: Download,
    onTap: () => openRelease(latest),
  });
}

if (inTauri) {
  setTimeout(() => void automaticCheck(), FIRST);
  setInterval(() => void automaticCheck(), EVERY);
}
