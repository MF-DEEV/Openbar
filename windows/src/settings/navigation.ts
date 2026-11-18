// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Moving to another page of the settings window from inside one, as the
// Sessions options' "Open Hooks" does (#236).

export const OPEN_PAGE_EVENT = "openbar:settings-page";

export function openSettingsPage(id: string): void {
  window.dispatchEvent(new CustomEvent<string>(OPEN_PAGE_EVENT, { detail: id }));
}
