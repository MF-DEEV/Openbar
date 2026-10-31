// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../i18n/index";
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
  useLocale();
  const enabled = useSessions((s) => !!s.autoAllow[session.id]);
  if (session.provider !== "codex" || (activeOnly && !enabled)) return null;
  if (activeOnly) {
    return (
      <div className="s-auto-allow-status" role="group" aria-label={t("windows.sessions.autoacceptActive")}>
        <span className="s-auto-allow-label">
          <BadgeCheck size={16} />
          <span>{t("notchClaudeSessions.alwaysOn")}</span>
        </span>
        <button
          type="button"
          className="s-auto-allow-cancel"
          aria-label={t("windows.sessions.cancelAutoaccept")}
          title={t("windows.sessions.turnOffAutoacceptForThisTask")}
          onClick={() => useSessions.setState((s) => ({ autoAllow: { ...s.autoAllow, [session.id]: false } }))}
        >
          {t("clipboard.cancel")}
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
      title={t("windows.sessions.ctrlTAutomaticallyAllowRequestsForThisCodex")}
      onClick={() => useSessions.getState().toggleAutoAllow(session.id)}
    >
      <BadgeCheck size={12} />
      {enabled ? t("notchClaudeSessions.alwaysOn") : t("notchClaudeSessions.alwaysAllow")}
    </button>
  );
}
