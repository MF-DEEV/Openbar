// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { BadgeCheck } from "lucide-react";
import { useSessions } from "./store";
import type { Session } from "./types";

export function AutoAllowButton({ session, disabled = false }: { session: Session; disabled?: boolean }) {
  const enabled = useSessions((s) => !!s.autoAllow[session.id]);
  if (session.provider !== "codex") return null;
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
