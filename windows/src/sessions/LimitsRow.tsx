// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Claude's and Codex's plan limits above the Sessions list
// (NotchSessionLimitsRow.swift): each agent's session and weekly windows as a
// thin meter, a share and the time until it renews. Quiet while there is
// room, colored as a window nears its end. The reading is agent_usage's;
// nothing shows until it has one.

import { useLocale } from "../i18n";

import { RotateCw } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { displayedReset, limitsAreStale, limitShare, usedFraction } from "../agents/model";
import { duration, limitTint, percent, PROVIDER_NAMES, PROVIDER_TINTS } from "../agents/format";
import { Meter } from "../agents/Parts";
import { useAgents } from "../agents/store";
import { A } from "../agents/strings";
import type { LimitWindow } from "../agents/types";
import { useClock } from "../agents/useClock";
import { useIsland } from "../island/store";
import { ClaudeMark, CodexMark } from "./glyphs";
import { limitGroups, type LimitGroup } from "./limits";
import { S } from "./strings";
import { useSessions } from "./store";

/** A small ring filled by `value`, 0…1, in the current color. */
export function LimitRing({ value, size }: { value: number; size: number }) {
  const fraction = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
  return (
    <svg width={size} height={size} viewBox="0 0 14 14" aria-hidden>
      <circle cx="7" cy="7" r="5" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.2" />
      <circle
        cx="7"
        cy="7"
        r="5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        pathLength="1"
        strokeDasharray={`${fraction} 1`}
        transform="rotate(-90 7 7)"
      />
    </svg>
  );
}

/** "5h" or "7d", the window's length; the kind's name when it has none. */
function windowName(window: LimitWindow): string {
  if (window.minutes && window.minutes > 0) return duration(window.minutes * 60, 1);
  return window.kind === "weekly" ? A.weekly : A.session;
}

/** How much fits: every agent on one line with its account, then a line
 * each, then a line each without the reset countdowns. */
type Fit = "line" | "stacked" | "shares";

export function LimitsRow() {
  useLocale();
  const shows = useSessions((s) => s.prefs.limits !== false);
  const snapshot = useAgents((s) => s.snapshot);
  const now = useClock(30_000);
  const groups = shows ? limitGroups(snapshot, now) : [];
  const [fit, setFit] = useState<Fit>("line");
  const row = useRef<HTMLDivElement>(null);
  const key = groups.map((g) => `${g.provider}:${g.windows.length}:${snapshot.accounts?.[g.provider] ?? ""}`).join(",");
  // Steps down until the row fits the page, the way ViewThatFits does on the Mac.
  useLayoutEffect(() => setFit("line"), [key]);
  useLayoutEffect(() => {
    const el = row.current;
    if (!el) return;
    const over = [el, ...el.querySelectorAll<HTMLElement>(".s-limits-chip")].some(
      (n) => n.scrollWidth > n.clientWidth + 1,
    );
    if (!over) return;
    if (fit === "line") setFit("stacked");
    else if (fit === "stacked") setFit("shares");
  });
  if (groups.length === 0) return null;
  return (
    <div className={`s-limits${fit === "line" ? "" : " is-stacked"}`} ref={row} role="group" aria-label={S.limitsTitle}>
      {groups.map((group) => (
        <Chip
          key={group.provider}
          group={group}
          now={now}
          fit={fit}
          stale={staleOf(group, snapshot, now)}
          account={accountOf(group.provider, snapshot)}
        />
      ))}
    </div>
  );
}

/** The account the limits are of: its email and plan, as the agent's own
 * files on this computer say; null while neither is known. */
function accountOf(provider: "claude" | "codex", snapshot: ReturnType<typeof useAgents.getState>["snapshot"]) {
  const parts = [snapshot.accounts?.[provider], snapshot.plans[provider]?.name].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

function staleOf(group: LimitGroup, snapshot: ReturnType<typeof useAgents.getState>["snapshot"], now: number) {
  return limitsAreStale(snapshot.limits[group.provider], group.provider, now);
}

function Chip(props: { group: LimitGroup; now: number; fit: Fit; stale: boolean; account: string | null }) {
  useLocale();
  const { group, now, fit, stale, account } = props;
  const Mark = group.provider === "codex" ? CodexMark : ClaudeMark;
  return (
    <span className={`s-limits-chip${stale ? " is-stale" : ""}`}>
      <Mark size={10} />
      {account ? (
        <span className="s-limits-account" title={account}>
          {account}
        </span>
      ) : null}
      {group.windows.map((window) => (
        <Window key={window.id} provider={group.provider} window={window} now={now} resets={fit !== "shares"} />
      ))}
    </span>
  );
}

function Window(props: { provider: "claude" | "codex"; window: LimitWindow; now: number; resets: boolean }) {
  useLocale();
  const display = useIsland((s) => s.settings.agentsLimitDisplay);
  const { window, provider, now } = props;
  const used = usedFraction(window);
  const value = display === "used" ? used : 1 - used;
  const tint = limitTint(provider, used);
  // Under the warning colors it stays quiet; past them the share takes the color.
  const urgent = tint !== PROVIDER_TINTS[provider];
  const reset = displayedReset(window, now);
  const name = windowName(window);
  const help = [
    `${PROVIDER_NAMES[provider]} · ${name}`,
    limitShare(window, "used"),
    limitShare(window, "remaining"),
    reset ? S.resets(new Date(reset).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })) : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <span className="s-limits-window" title={help}>
      <span className="s-limits-name">{name}</span>
      <span className="s-limits-meter">
        <Meter value={value} tint={tint} dimmed={!urgent} height={3} />
      </span>
      <span className="s-limits-share" style={urgent ? { color: tint } : undefined}>
        {percent(value)}
      </span>
      {props.resets && reset ? (
        <span className="s-limits-reset">
          <RotateCw size={8} strokeWidth={2.5} />
          {duration(Math.max(60, (reset - now) / 1000), 1)}
        </span>
      ) : null}
    </span>
  );
}
