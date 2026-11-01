// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while sessions run (NotchClaudeSessionStrip.swift; spec
// §4.2): the most pressing phase on one side of the camera, how many
// sessions are in each phase on the other. Clicking opens the Sessions page,
// on the first waiting session when there is one.

import { useLocale } from "../i18n";

import { CheckCircleFill, ClaudeMark, CodexMark, GaugeFill, HandFill, MARK_SCALE, SparkleFill } from "./glyphs";
import { edgeInset, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { useAgents } from "../agents/store";
import { limitTint } from "../agents/format";
import { LimitRing } from "./LimitsRow";
import { CLAUDE_TINT, CODEX_TINT, FINISHED_TINT, LIMITED_TINT, WAITING_TINT } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import { presentedSnapshot } from "./presentation";
import { currentStripLimit, type StripLimit } from "./stripLimit";
import type { Counts } from "./types";

/** min(15, strip − 7): 15 pt in the Windows bar's 24 pt strip. */
export const stripTextSize = (stripHeight: number) => Math.min(15, stripHeight - 7);
const MARK = 14;

/** Waiting, then each agent at work, then the ones a plan limit stopped,
 * then finished. The mark of who works shows once, beside the camera; the
 * counts use the phase's sparkle in each agent's color, as the Mac's counts
 * use the phase symbol, and a coral gauge for the stopped ones. */
export function countItems(counts: Counts) {
  return [
    { n: counts.waiting, Icon: HandFill, tint: WAITING_TINT },
    { n: counts.working - counts.workingCodex, Icon: SparkleFill, tint: CLAUDE_TINT },
    { n: counts.workingCodex, Icon: SparkleFill, tint: CODEX_TINT },
    { n: counts.limited ?? 0, Icon: GaugeFill, tint: LIMITED_TINT },
    { n: counts.finished, Icon: SparkleFill, tint: FINISHED_TINT },
  ].filter((i) => i.n > 0);
}

/** The ring beside the limit's share on the strip. */
export const limitRing = (size: number) => Math.round(size * 0.75);

/** How wide the counts draw (NotchClaudeSessionsSupport.stripReadingWidth),
 * and the limit's ring and share after them. */
export function readingWidth(counts: Counts, size: number, limit?: string | null): number {
  const list = countItems(counts);
  const digits = list.reduce((sum, i) => sum + String(i.n).length, 0);
  let width = list.length * (size * 0.9 + 2) + digits * size * 0.62 + Math.max(0, list.length - 1) * 6;
  if (limit) width += (list.length ? 6 : 0) + limitRing(size) + 2 + limit.length * size * 0.62;
  return width;
}

/** Whose marks show while sessions work, front first: the one agent at
 * work, or both stacked with Claude's in front when Claude and Codex work at
 * once (NotchClaudeSessionsSupport.stripAgents). Empty when none do. */
export function stripAgents(counts: Counts): ("claude" | "codex")[] {
  const working = Math.max(0, counts.working);
  const codex = Math.min(Math.max(0, counts.workingCodex), working);
  const agents: ("claude" | "codex")[] = [];
  if (working > codex) agents.push("claude");
  if (codex > 0) agents.push("codex");
  return agents;
}

const SPARK = Math.round(MARK * MARK_SCALE.claude);
const CLOUD = Math.round(MARK * MARK_SCALE.codex);
/** The spark's disc of the strip's black, a ring past its rays. */
const DISC = SPARK + 2;
/** Where the cloud behind starts, from the spark's left: past the disc's
 * middle, so about three quarters of it shows. */
export const STACK_OFFSET = 17;

/** Stacked while a session works for each agent and none waits; a waiting
 * hand or a finished check takes the mark's place, as on the Mac. */
const stacks = (counts: Counts) => counts.waiting <= 0 && stripAgents(counts).length > 1;

/** How wide the marks draw: one mark, or the spark and the cloud past it. */
export function marksWidth(counts: Counts): number {
  return stacks(counts) ? STACK_OFFSET + CLOUD : MARK;
}

/** The wing each side needs, clamped to 44…80 like the agents' strip. */
export function stripWing(counts: Counts, stripHeight: number, limit?: string | null): number {
  const size = stripTextSize(stripHeight);
  const box = { width: 300, height: stripHeight };
  const text = edgeInset(box, size * 0.72, 0) + readingWidth(counts, size, limit);
  const mark = edgeInset(box, 18, 9) + marksWidth(counts);
  return Math.min(80, Math.max(44, Math.ceil(Math.max(text, mark))));
}

/** Claude's spark on its disc with Codex's cloud showing past it; the cloud
 * slides out from behind when Codex starts. */
function StackedMarks() {
  return (
    <span className="s-strip-stack" style={{ width: STACK_OFFSET + CLOUD, height: DISC }} aria-hidden>
      <span className="s-strip-behind" style={{ left: STACK_OFFSET, marginTop: -CLOUD / 2 }}>
        <CodexMark size={CLOUD} working />
      </span>
      <span className="s-strip-front" style={{ width: DISC, height: DISC, marginTop: -DISC / 2, left: -1 }}>
        <ClaudeMark size={SPARK} working />
      </span>
    </span>
  );
}

export function SessionsStrip({ geometry, size }: { geometry: Geometry; size: Size }) {
  useLocale();
  const counts = useSessions((s) => presentedSnapshot(s).counts);
  // Read again when the limits, the settings or the sessions change.
  useAgents((s) => s.snapshot);
  useIsland((s) => s.settings);
  useSessions((s) => s.snapshot);
  const limit = currentStripLimit();
  const textSize = stripTextSize(geometry.stripHeight);
  const capsule = geometry.floatingGap !== null;
  const iconInset = capsule ? 0 : edgeInset(size, 18, 9, 5, geometry.input);
  const textInset = capsule ? 0 : edgeInset(size, textSize * 0.72, 0, 5, geometry.input);
  const wing = capsule ? undefined : (size.width - geometry.cameraWidth) / 2;

  const open = () => {
    const waiting = presentedSnapshot(useSessions.getState()).sessions.find(
      (s) => s.permission || s.phase.startsWith("waiting"),
    );
    const opened = useIsland.getState().activateActivity("sessions");
    if (opened && waiting) useSessions.getState().focus(waiting.id);
  };

  // Claude's spark while any Claude session works, Codex's cloud when only
  // Codex does; both stacked while both do.
  const working = counts.working > 0;
  const stacked = stacks(counts);
  // Nothing works and the plan stopped something: that comes before a result.
  const limited = !working && counts.waiting <= 0 && (counts.limited ?? 0) > 0;
  const Mark =
    counts.waiting > 0
      ? HandFill
      : limited
        ? GaugeFill
        : !working
          ? CheckCircleFill
          : counts.workingCodex === counts.working
            ? CodexMark
            : ClaudeMark;
  const markTint = counts.waiting > 0 ? WAITING_TINT : working ? undefined : limited ? LIMITED_TINT : FINISHED_TINT;
  const markScale = counts.waiting > 0 || !working ? 1 : MARK_SCALE[Mark === CodexMark ? "codex" : "claude"];

  return (
    <button
      type="button"
      className={`s-strip${capsule ? " is-capsule" : ""}`}
      style={{ width: size.width, height: size.height, padding: capsule ? "0 14px" : undefined }}
      aria-label={S.title}
      onClick={open}
    >
      <span className="s-strip-wing" style={{ width: wing, paddingLeft: iconInset, justifyContent: "flex-start" }}>
        {stacked ? (
          <StackedMarks />
        ) : (
          <Mark
            size={Math.round(MARK * markScale)}
            strokeWidth={2.5}
            style={markTint ? { color: markTint } : undefined}
            working={working}
          />
        )}
      </span>
      {capsule ? null : <span style={{ width: geometry.cameraWidth, flex: "none" }} />}
      <span
        className="s-strip-wing"
        style={{ width: wing, paddingRight: textInset, justifyContent: "flex-end", fontSize: textSize }}
      >
        {countItems(counts).map(({ n, Icon, tint }, i) => (
          <span key={i} className="s-strip-count" style={{ color: tint }}>
            <Icon size={Math.round(textSize * 0.7)} strokeWidth={3} />
            {n}
          </span>
        ))}
        {limit ? <StripLimitReading limit={limit} textSize={textSize} /> : null}
      </span>
    </button>
  );
}

/** The limit past the warning threshold, after the counts: a ring and its
 * share in the window's warning color. */
function StripLimitReading({ limit, textSize }: { limit: StripLimit; textSize: number }) {
  useLocale();
  const display = useIsland((s) => s.settings.agentsLimitDisplay);
  const used = Math.min(1, Math.max(0, limit.window.usedPercent / 100));
  const tint = limitTint(limit.provider, used);
  return (
    <span className="s-strip-count s-strip-limit" style={{ color: tint }}>
      <LimitRing value={display === "used" ? used : 1 - used} size={limitRing(textSize)} />
      <span style={{ fontSize: textSize * 0.85 }}>{limit.share}</span>
    </span>
  );
}
