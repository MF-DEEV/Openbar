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
  if (activeOnly) {
    return (
      <div className="s-auto-allow-status" role="group" aria-label="Autoaccept active">
        <span className="s-auto-allow-label">
          <BadgeCheck size={16} />
          <span>Always: ON</span>
        </span>
        <button
          type="button"
          className="s-auto-allow-cancel"
          aria-label="Cancel autoaccept"
          title="Turn off autoaccept for this task."
          onClick={() => useSessions.setState((s) => ({ autoAllow: { ...s.autoAllow, [session.id]: false } }))}
        >
          Cancel
        </button>
      </div>
    );
  }
  return (
    <button
      type="button"
      className={`s-button ${enabled ? "is-allow" : "is-secondary"}`}
      aria-pressed={enabled}
      aria-keyshortcuts="Control+t"
      disabled={disabled || session.phase === "finished" || session.phase === "idle"}
      title="Ctrl+T — automatically allow requests for this Codex task. Resets when the task finishes. Click again to turn off."
      onClick={() => useSessions.getState().toggleAutoAllow(session.id)}
    >
      <BadgeCheck size={12} />
      {enabled ? "Always: ON" : "Always"}
    </button>
  );
}
