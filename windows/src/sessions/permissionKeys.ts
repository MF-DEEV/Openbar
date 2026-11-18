// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useIsland } from "../island/store";
import { settled, useSessions } from "./store";
import { codexServer, type Session } from "./types";
import { asks, showing } from "./queue";

export type PermissionKey = "y" | "n" | "always";

export async function performPermissionKey(session: Session, key: PermissionKey): Promise<boolean> {
  if (key !== "n" && session.permission?.command?.requiresInspection) return false;
  if (key === "always" && codexServer(session)) {
    useSessions.getState().toggleAutoAllow(session.id);
    return true;
  }
  return useSessions.getState().decide(session, key === "always" ? "always" : key === "y" ? "allow" : "deny");
}

export function permissionKeyTarget(sessionId: string, requestId: string) {
  const island = useIsland.getState();
  const state = useSessions.getState();
  if (
    !island.settings.enabled ||
    !island.expanded ||
    island.selected !== "sessions" ||
    island.showingSections ||
    state.focused !== sessionId ||
    state.renaming ||
    state.wardrobe
  )
    return undefined;
  // The ask on the page, the first or one the queue opened it on (#269).
  const session = state.snapshot.sessions.find((s) => s.id === sessionId && s.phase === "waitingPermission");
  const ask = session && asks(session).find((p) => p.requestId === requestId);
  if (!session || !ask || ask.questions.length) return undefined;
  return showing(session, requestId);
}

/** Answer only the request currently visible on this session's page. */
export function handlePermissionKey(event: KeyboardEvent, sessionId: string, requestId: string): boolean {
  if (
    event.defaultPrevented ||
    event.isComposing ||
    event.repeat ||
    !event.ctrlKey ||
    event.altKey ||
    event.metaKey ||
    event.shiftKey
  )
    return false;
  const key = event.key.toLowerCase();
  if (key !== "y" && key !== "n" && key !== "t") return false;
  const state = useSessions.getState();
  const target = event.target as HTMLElement | null;
  if (
    target?.closest?.("input, textarea, [contenteditable]:not([contenteditable='false'])") ||
    (typeof document !== "undefined" && document.querySelector("dialog[open]"))
  )
    return false;
  const session = permissionKeyTarget(sessionId, requestId);
  if (!session) return false;
  if (key === "t" && !codexServer(session) && !session.permission!.offersAlways) return false;
  event.preventDefault();
  event.stopPropagation();
  // Consume the shortcut during the same settling interval as the buttons.
  if (settled(state, session)) void performPermissionKey(session, key === "t" ? "always" : key);
  return true;
}
