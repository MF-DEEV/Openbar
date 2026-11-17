// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// Sessions in their agent's no-ask mode (#234): the mark beside the name,
// and what the island let through, on the session's page. The same as the
// Mac's NotchSessionAutoModeViews.

import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { useLocale } from "../i18n";
import { providerTint } from "./format";
import { S } from "./strings";
import type { LetThrough, Provider, Session } from "./types";

function Forward({ size = 9 }: { size?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden>
      <path d="M2 3.2v9.6L7.6 8zM8.4 3.2v9.6L14 8z" fill="currentColor" />
    </svg>
  );
}

/** "auto" in the agent's color; its title says what it means. */
export function AutoMark({ provider }: { provider: Provider }) {
  useLocale();
  const tint = providerTint(provider);
  const help = S.autoMarkHelp(provider);
  return (
    <span
      className="s-remote-mark s-auto-mark"
      style={{ color: tint, background: `color-mix(in srgb, ${tint} 16%, transparent)` }}
      title={help}
      aria-label={help}
    >
      <Forward />
      {S.autoMark}
    </span>
  );
}

const FOLDED = 5;

function time(at: number, locale: string): string {
  try {
    return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(new Date(at * 1000));
  } catch {
    return new Date(at * 1000).toLocaleTimeString();
  }
}

/** What the island allowed without asking, newest first. */
export function LetThroughList({ session, enlarged = false }: { session: Session; enlarged?: boolean }) {
  const locale = useLocale();
  const [expanded, setExpanded] = useState(false);
  const all: LetThrough[] = [...(session.letThrough ?? [])].reverse();
  if (all.length === 0) return null;
  const shown = all.slice(0, enlarged && expanded ? all.length : FOLDED);
  const tint = providerTint(session.provider);
  return (
    <div className="s-section s-let-through">
      <div className="s-section-title">{S.letThroughTitle}</div>
      {shown.map((entry) => (
        <div key={entry.id} className="s-let-through-row">
          <span className="s-let-through-mark" style={{ color: tint }}>
            <Forward size={8} />
          </span>
          <span className="s-let-through-tool">{entry.tool}</span>
          {entry.summary ? <span className="s-let-through-summary">{entry.summary}</span> : null}
          {entry.reason === "alwaysProceed" ? <span className="s-tertiary">{S.letThroughAlways}</span> : null}
          <span className="s-clock is-small">{time(entry.at, locale)}</span>
        </div>
      ))}
      {all.length > FOLDED ? (
        enlarged ? (
          <button type="button" className="s-fold-list" onClick={() => setExpanded(!expanded)} aria-expanded={expanded}>
            <ChevronDown size={10} className={expanded ? "is-open" : ""} />
            {expanded ? S.showLess : S.showAll(all.length)}
          </button>
        ) : (
          <div className="s-tertiary">{S.moreLetThrough(all.length - FOLDED)}</div>
        )
      ) : null}
    </div>
  );
}
