// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useIsland } from "../island/store";
import { settled, useSessions } from "./store";

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
  return state.snapshot.sessions.find(
    (s) =>
      s.id === sessionId &&
      s.phase === "waitingPermission" &&
      s.permission?.requestId === requestId &&
      !s.permission.questions.length,
  );
}

/** Answer only the request currently visible on this session's page. */
export function handlePermissionKey(event: KeyboardEvent, sessionId: string, requestId: string): boolean {
  if (
    event.defaultPrevented ||
    event.isComposing ||
    event.repeat ||
    !event.ctrlKey ||
    event.altKey ||
    event.shiftKey ||
    event.metaKey
  )
    return false;
  const key = event.key.toLowerCase();
  if (key !== "y" && key !== "n") return false;
  const state = useSessions.getState();
  const target = event.target as HTMLElement | null;
  if (
    target?.closest?.("input, textarea, [contenteditable]:not([contenteditable='false'])") ||
    (typeof document !== "undefined" && document.querySelector("dialog[open]"))
  )
    return false;
  const session = permissionKeyTarget(sessionId, requestId);
  if (!session) return false;
  event.preventDefault();
  event.stopPropagation();
  // Consume the shortcut during the same settling interval as the buttons.
  if (settled(state, session)) void state.decide(session, key === "y" ? "allow" : "deny");
  return true;
}
