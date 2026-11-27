// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Moving to another page of the settings window from inside one, as the
// Sessions options' "Open Hooks" does (#236).

export const OPEN_PAGE_EVENT = "openbar:settings-page";

export function openSettingsPage(id: string): void {
  window.dispatchEvent(new CustomEvent<string>(OPEN_PAGE_EVENT, { detail: id }));
}

// A section of a page brought into view from outside the window, as the
// sessions' tray menu does for theirs (#331). The app sets
// `window.__openbarSection` before the page is drawn, or sends the event to a
// page already shown; either is taken once.
export const OPEN_SECTION_EVENT = "openbar:settings-section";

declare global {
  interface Window {
    __openbarSection?: string;
  }
}

/** True once for the section asked for before its page was drawn. */
export function takeSection(id: string): boolean {
  if (window.__openbarSection !== id) return false;
  window.__openbarSection = undefined;
  return true;
}

/** Calls `reveal` when the section is asked for while its page shows. */
export function onSection(id: string, reveal: () => void): () => void {
  const handler = (event: Event) => {
    if ((event as CustomEvent<string>).detail !== id) return;
    window.__openbarSection = undefined;
    reveal();
  };
  window.addEventListener(OPEN_SECTION_EVENT, handler);
  return () => window.removeEventListener(OPEN_SECTION_EVENT, handler);
}
