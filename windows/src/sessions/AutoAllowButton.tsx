// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { BadgeCheck } from "lucide-react";
import { useSessions } from "./store";
import type { Session } from "./types";

export function AutoAllowButton({
  session,
  disabled = false,
  activeOnly = false,
}: {
  session: Session;
  disabled?: boolean;
  activeOnly?: boolean;
}) {
  const enabled = useSessions((s) => !!s.autoAllow[session.id]);
  if (session.provider !== "codex" || (activeOnly && !enabled)) return null;
  return (
    <button
      type="button"
      className={`s-button ${enabled ? "is-allow" : "is-secondary"}`}
      aria-pressed={enabled}
      aria-keyshortcuts={activeOnly ? undefined : "Control+t"}
      aria-label={activeOnly ? "Cancel autoaccept" : undefined}
      disabled={disabled || session.phase === "finished" || session.phase === "idle"}
      title={
        activeOnly
          ? "Click to cancel autoaccept for this task."
          : "Ctrl+T — automatically allow requests for this Codex task. Resets when the task finishes. Click again to turn off."
      }
      onClick={() =>
        activeOnly
          ? useSessions.setState((s) => ({ autoAllow: { ...s.autoAllow, [session.id]: false } }))
          : useSessions.getState().toggleAutoAllow(session.id)
      }
    >
      <BadgeCheck size={12} />
      {activeOnly ? "Always: ON · Cancel" : enabled ? "Always: ON" : "Always"}
    </button>
  );
}
