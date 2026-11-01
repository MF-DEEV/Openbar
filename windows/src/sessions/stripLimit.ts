// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The plan limit the Sessions strip shows now, read from the stores
// (NotchSessionLimitsSupport.currentStripLimit on the Mac).

import { useAgents } from "../agents/store";
import type { LimitWindow, Provider as AgentProvider } from "../agents/types";
import { modulesOf, useIsland } from "../island/store";
import { stripLimit, stripShare } from "./limits";
import { useSessions } from "./store";

/** Sessions shows the plan limits: the section is on and its switch too. */
export function sessionLimitsOn(): boolean {
  const island = useIsland.getState();
  return (
    island.settings.enabled && modulesOf(island).includes("sessions") && useSessions.getState().prefs.limits !== false
  );
}

export interface StripLimit {
  provider: AgentProvider;
  window: LimitWindow;
  share: string;
}

/** Among the agents of the sessions listed, as the AI strip's settings choose
 * it, past the warning threshold; null otherwise. */
export function currentStripLimit(now = Date.now()): StripLimit | null {
  if (!sessionLimitsOn()) return null;
  const { settings } = useIsland.getState();
  const agents = new Set(useSessions.getState().snapshot.sessions.map((s) => s.provider));
  const threshold = settings.agentsLimitAlert ? settings.agentsLimitThreshold : 80;
  const limit = stripLimit(useAgents.getState().snapshot, agents, settings.agentsLimitFocus, threshold, now);
  return limit ? { ...limit, share: stripShare(limit.window, settings.agentsLimitDisplay) } : null;
}
