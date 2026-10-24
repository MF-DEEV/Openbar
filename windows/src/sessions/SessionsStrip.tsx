// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while sessions run (NotchClaudeSessionStrip.swift; spec
// §4.2): the most pressing phase on one side of the camera, how many
// sessions are in each phase on the other. Clicking opens the Sessions page,
// on the first waiting session when there is one.

import { CheckCircleFill, HandFill, SparkleFill } from "./glyphs";
import { edgeInset, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { CLAUDE_TINT, FINISHED_TINT, WAITING_TINT } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import type { Counts } from "./types";

/** min(15, strip − 7): 15 pt in the Windows bar's 24 pt strip. */
export const stripTextSize = (stripHeight: number) => Math.min(15, stripHeight - 7);
const MARK = 14;

function items(counts: Counts) {
  return [
    { n: counts.waiting, Icon: HandFill, tint: WAITING_TINT },
    { n: counts.working, Icon: SparkleFill, tint: CLAUDE_TINT },
    { n: counts.finished, Icon: CheckCircleFill, tint: FINISHED_TINT },
  ].filter((i) => i.n > 0);
}

/** How wide the counts draw (NotchClaudeSessionsSupport.stripReadingWidth). */
export function readingWidth(counts: Counts, size: number): number {
  const list = items(counts);
  if (list.length === 0) return 0;
  const digits = list.reduce((sum, i) => sum + String(i.n).length, 0);
  return list.length * (size * 0.9 + 2) + digits * size * 0.62 + (list.length - 1) * 6;
}

/** The wing each side needs, clamped to 44…80 like the agents' strip. */
export function stripWing(counts: Counts, stripHeight: number): number {
  const size = stripTextSize(stripHeight);
  const box = { width: 300, height: stripHeight };
  const text = edgeInset(box, size * 0.72, 0) + readingWidth(counts, size);
  const mark = edgeInset(box, 18, 9) + MARK;
  return Math.min(80, Math.max(44, Math.ceil(Math.max(text, mark))));
}

export function SessionsStrip({ geometry, size }: { geometry: Geometry; size: Size }) {
  const counts = useSessions((s) => s.snapshot.counts);
  const textSize = stripTextSize(geometry.stripHeight);
  const capsule = geometry.floatingGap !== null;
  const iconInset = capsule ? 0 : edgeInset(size, 18, 9);
  const textInset = capsule ? 0 : edgeInset(size, textSize * 0.72, 0);
  const wing = capsule ? undefined : (size.width - geometry.cameraWidth) / 2;

  const open = () => {
    const waiting = useSessions.getState().snapshot.sessions.find((s) => s.permission || s.phase.startsWith("waiting"));
    useIsland.getState().open("sessions");
    if (waiting) useSessions.getState().focus(waiting.id);
  };

  const Mark = counts.waiting > 0 ? HandFill : counts.working > 0 ? SparkleFill : CheckCircleFill;
  const markTint = counts.waiting > 0 ? WAITING_TINT : counts.working > 0 ? CLAUDE_TINT : FINISHED_TINT;

  return (
    <button
      type="button"
      className={`s-strip${capsule ? " is-capsule" : ""}`}
      style={{ width: size.width, height: size.height, padding: capsule ? "0 14px" : undefined }}
      aria-label={S.title}
      onClick={open}
    >
      <span className="s-strip-wing" style={{ width: wing, paddingLeft: iconInset, justifyContent: "flex-start" }}>
        <Mark size={MARK} strokeWidth={2.5} style={{ color: markTint }} />
      </span>
      {capsule ? null : <span style={{ width: geometry.cameraWidth, flex: "none" }} />}
      <span
        className="s-strip-wing"
        style={{ width: wing, paddingRight: textInset, justifyContent: "flex-end", fontSize: textSize }}
      >
        {items(counts).map(({ n, Icon, tint }, i) => (
          <span key={i} className="s-strip-count" style={{ color: tint }}>
            <Icon size={Math.round(textSize * 0.7)} strokeWidth={3} />
            {n}
          </span>
        ))}
      </span>
    </button>
  );
}
