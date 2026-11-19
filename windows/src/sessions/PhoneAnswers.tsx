// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// What was answered from the phone for a session (#214), newest first, so
// its page tells which answers came from Telegram. The same as the Mac's
// NotchSessionPhoneAnswerList.

import { Smartphone } from "lucide-react";
import { t, useLocale } from "../i18n";
import { providerTint } from "./format";
import type { PhoneAnswer, Session } from "./types";

const SHOWN = 5;

function time(at: number, locale: string): string {
  try {
    return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(new Date(at * 1000));
  } catch {
    return new Date(at * 1000).toLocaleTimeString();
  }
}

export function verdictText(verdict: PhoneAnswer["verdict"]): string {
  switch (verdict) {
    case "allowed":
      return t("windows.sessions.phoneAllowed");
    case "always":
      return t("windows.sessions.phoneAlways");
    case "denied":
      return t("windows.sessions.phoneDenied");
    case "planApproved":
      return t("windows.sessions.phonePlanApproved");
    case "keptPlanning":
      return t("windows.sessions.phoneKeptPlanning");
    case "answered":
      return t("windows.sessions.phoneAnswered");
    default:
      return t("windows.sessions.phoneSkipped");
  }
}

export function PhoneAnswerList({ session }: { session: Session }) {
  const locale = useLocale();
  const all = [...(session.phoneAnswers ?? [])].reverse();
  if (all.length === 0) return null;
  const tint = providerTint(session.provider);
  return (
    <div className="s-section s-let-through">
      <div className="s-section-title">{t("windows.sessions.phoneTitle")}</div>
      {all.slice(0, SHOWN).map((entry) => (
        <div key={`${entry.id}:${entry.at}`} className="s-let-through-row">
          <span className="s-let-through-mark" style={{ color: tint }}>
            <Smartphone size={9} />
          </span>
          <span className="s-let-through-tool">{entry.tool}</span>
          {entry.summary ? <span className="s-let-through-summary">{entry.summary}</span> : null}
          <span className="s-tertiary">{verdictText(entry.verdict)}</span>
          <span className="s-clock is-small">{time(entry.at, locale)}</span>
        </div>
      ))}
    </div>
  );
}
