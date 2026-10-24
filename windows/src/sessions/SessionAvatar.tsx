// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's face: its Clawd in the outfit drawn for it (or chosen in the
// wardrobe), moving with its phase; a round phase badge with buddies off.

import { PhaseBadge } from "./Avatar";
import { Clawd } from "./clawd/Clawd";
import { resolvedCostume, subagentKey } from "./clawd/costume";
import { CLAWD_SIZES, motionForPhase, motionForSubagent } from "./clawd/sprite";
import { dozing } from "./format";
import { useSessions } from "./store";
import type { Session, Subagent } from "./types";

const SIZES = {
  row: { ...CLAWD_SIZES.listRow, badge: 26 },
  page: { ...CLAWD_SIZES.pageHeader, badge: 30 },
  large: { ...CLAWD_SIZES.largeHeader, badge: 34 },
};

export function SessionAvatar(props: { session: Session; now: number; lookedAt?: number; size: keyof typeof SIZES }) {
  const { session } = props;
  const s = SIZES[props.size];
  const buddies = useSessions((st) => st.prefs.buddies);
  const kept = useSessions((st) => st.costumes[session.id]);
  const asleep = dozing(session, props.now, props.lookedAt);
  if (!buddies) {
    return (
      <span className="s-avatar" style={{ width: s.width, height: s.height, opacity: asleep ? 0.65 : 1 }}>
        <PhaseBadge phase={session.phase} provider={session.provider} size={s.badge} />
      </span>
    );
  }
  return (
    <span className="s-avatar" style={{ width: s.width, height: s.height }}>
      <Clawd
        character={session.provider === "codex" ? "codex" : "clawd"}
        costume={resolvedCostume(session.id, kept, null, session.provider === "codex" ? "codex" : "clawd")}
        motion={motionForPhase(session.phase, asleep)}
        seed={session.id}
        since={session.phaseSince * 1000}
        width={s.width}
        height={s.height}
        style={{ opacity: asleep ? 0.65 : 1 }}
      />
    </span>
  );
}

/** A subagent's own Clawd: its own lot, the shell from its definition's `color:`. */
export function SubagentAvatar({ session, agent }: { session: Session; agent: Subagent }) {
  const key = subagentKey(session.id, agent.id);
  const kept = useSessions((st) => st.costumes[key]);
  const running = agent.finished === undefined;
  return (
    <Clawd
      costume={resolvedCostume(key, kept, agent.color)}
      motion={motionForSubagent(running)}
      seed={key}
      since={agent.started * 1000}
      {...CLAWD_SIZES.subagentRow}
      style={{ opacity: running ? 1 : 0.5, flex: "none" }}
    />
  );
}
