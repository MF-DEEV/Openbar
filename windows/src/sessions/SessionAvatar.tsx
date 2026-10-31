// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's face: its Clawd in the outfit drawn for it (or chosen in the
// wardrobe), moving with its phase; a round phase badge with buddies off.
// A sleeping session's row fades as a whole (SessionsList), not the face.

import { useLocale } from "../i18n";

import { PhaseBadge, phaseIcon } from "./Avatar";
import { Clawd } from "./clawd/Clawd";
import { resolvedCostume, subagentKey } from "./clawd/costume";
import { CLAWD_SIZES, motionForNotice, motionForPhase, motionForSubagent } from "./clawd/sprite";
import { dozing, phaseTint } from "./format";
import { useSessions } from "./store";
import type { Provider, Session, SessionEvent, Subagent } from "./types";

const SIZES = {
  row: { ...CLAWD_SIZES.listRow, badge: 26 },
  page: { ...CLAWD_SIZES.pageHeader, badge: 30 },
  large: { ...CLAWD_SIZES.largeHeader, badge: 34 },
};

export function SessionAvatar(props: { session: Session; now: number; lookedAt?: number; size: keyof typeof SIZES }) {
  useLocale();
  const { session } = props;
  const s = SIZES[props.size];
  const buddies = useSessions((st) => st.prefs.buddies);
  const kept = useSessions((st) => st.costumes[session.id]);
  if (!buddies) {
    return (
      <span className="s-avatar" style={{ width: s.width, height: s.height }}>
        <PhaseBadge phase={session.phase} provider={session.provider} size={s.badge} />
      </span>
    );
  }
  return (
    <span className="s-avatar" style={{ width: s.width, height: s.height }}>
      <Clawd
        character={session.provider === "codex" ? "codex" : "clawd"}
        costume={resolvedCostume(session.id, kept, null, session.provider === "codex" ? "codex" : "clawd")}
        motion={motionForPhase(session.phase, dozing(session, props.now, props.lookedAt))}
        seed={session.id}
        since={session.phaseSince * 1000}
        width={s.width}
        height={s.height}
      />
    </span>
  );
}

/** A subagent's own Clawd: its own lot, the shell from its definition's `color:`. */
export function SubagentAvatar({ session, agent }: { session: Session; agent: Subagent }) {
  useLocale();
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

const NOTICE_MOTION = { needsPermission: "permission", needsInput: "input", finished: "finished" } as const;
const NOTICE_PHASE = {
  needsPermission: "waitingPermission",
  needsInput: "waitingInput",
  finished: "finished",
} as const;

/**
 * A session's Clawd in a notice about it (NotchClaudeNoticeClawd): hopping
 * while the session waits, cheering once its work is done. The strip has no
 * room above it, so no bubble shows. With buddies off, the notice's symbol in
 * the tint of its phase, as on the Mac.
 */
export function SessionNoticeMark(props: {
  session: { id: string; provider: Provider; event: SessionEvent["kind"] };
  height: number;
}) {
  useLocale();
  const { id, event } = props.session;
  const buddies = useSessions((st) => st.prefs.buddies);
  const kept = useSessions((st) => st.costumes[id]);
  // The notice's own agent stands in for a session that has already left the list.
  const live = useSessions((st) => st.snapshot.sessions.find((s) => s.id === id));
  const provider = live?.provider ?? props.session.provider;
  const character = provider === "codex" ? "codex" : "clawd";
  if (!buddies) {
    const phase = NOTICE_PHASE[event];
    const Icon = phaseIcon(phase, provider);
    // Both waits wear the amber of the badge; a finished turn its green. The
    // slot keeps the Clawd's width, as the Mac's markWidth does.
    return (
      <span style={{ display: "inline-flex", justifyContent: "center", width: CLAWD_SIZES.notice.width, flex: "none" }}>
        <Icon size={14} strokeWidth={2.5} style={{ color: phaseTint(phase) }} />
      </span>
    );
  }
  return (
    <Clawd
      character={character}
      costume={resolvedCostume(id, kept, null, character)}
      motion={motionForNotice(NOTICE_MOTION[event])}
      seed={id}
      since={live ? live.phaseSince * 1000 : null}
      cheer={live?.phase === "finished" ? live.phaseSince * 1000 : null}
      bubbles={false}
      width={CLAWD_SIZES.notice.width}
      height={Math.min(CLAWD_SIZES.notice.height, props.height)}
    />
  );
}
