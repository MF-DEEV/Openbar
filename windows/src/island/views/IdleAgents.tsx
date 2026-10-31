// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../../i18n/index";

import { cost, limitTint, percent } from "../../agents/format";
import { readingWidth } from "../../agents/AgentsStrip";
import { restingLimit, usedFraction } from "../../agents/model";
import { Mark } from "../../agents/Parts";
import { useAgents } from "../../agents/store";
import { PROVIDERS } from "../../agents/types";
import { useClock } from "../../agents/useClock";
import { useIsland } from "../store";

/** NotchAgentRestingWing: allowance or today's cost. Work and waiting
 * sessions retain their existing higher-priority status strips. */
export function IdleAgents({ leading, width }: { leading: boolean; width: number }) {
  useLocale();
  const snapshot = useAgents((s) => s.snapshot);
  const focus = useIsland((s) => s.settings.agentsLimitFocus);
  const display = useIsland((s) => s.settings.agentsLimitDisplay);
  const now = useClock(60_000);
  const limit = restingLimit(snapshot, focus, now);
  if (limit) {
    const used = usedFraction(limit.window);
    const value = display === "used" ? used : 1 - used;
    const tint = limitTint(limit.provider, used);
    return leading ? (
      <svg width="11" height="11" viewBox="0 0 14 14" aria-hidden style={{ color: tint }}>
        <circle cx="7" cy="7" r="5" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.2" />
        <circle
          cx="7"
          cy="7"
          r="5"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          pathLength="1"
          strokeDasharray={`${value} 1`}
          transform="rotate(-90 7 7)"
        />
      </svg>
    ) : (
      <span>{percent(value)}</span>
    );
  }
  const provider = PROVIDERS.find((provider) => snapshot.seen.includes(provider));
  if (!provider) return null;
  const label = cost(snapshot.periods.today.total.cost);
  // Match the Mac's minimumScaleFactor(0.7) without widening the resting wings.
  const fontSize = Math.max(6.3, Math.min(9, (9 * width) / Math.max(1, readingWidth(label, 9))));
  return leading ? <Mark provider={provider} size={10} /> : <span style={{ fontSize }}>{label}</span>;
}
