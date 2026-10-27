// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while an agent works (NotchAgentStrip.swift; spec 03
// §10.4): the marks of the agents at work on one side of the camera, the
// reading the person chose on the other. Clicking opens the AI page.

import { edgeInset, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { PROVIDER_NAMES, PROVIDER_TINTS } from "./format";
import { advancesWithClock, readingShape, stripReading, workingProviders } from "./model";
import { Glyph } from "./Parts";
import { useAgents } from "./store";
import type { AgentSnapshot, Provider } from "./types";
import { useClock } from "./useClock";

/** min(15, strip − 7), as the sessions strip. */
export const stripTextSize = (stripHeight: number) => Math.min(15, stripHeight - 7);
/** 14 pt with one agent at work, 11 pt with several. */
export const stripIconSize = (count: number, stripHeight: number) =>
  Math.min(count > 1 ? 11 : 14, Math.max(8, stripHeight - 6));
/** Each mark sits in a frame of size × 1.45 + 1, a point apart. */
export const marksWidth = (count: number, icon: number) => count * (icon * 1.45 + 1) + Math.max(0, count - 1);
/** Air between the camera and what sits beside it. */
const CAMERA_GAP = 6;

let canvas: CanvasRenderingContext2D | null | undefined;
/** The width the reading draws at `size`, measured on its shape so the strip never jitters. */
export function readingWidth(reading: string, size: number): number {
  const shape = readingShape(reading);
  if (canvas === undefined) {
    try {
      canvas = document.createElement("canvas").getContext("2d");
    } catch {
      canvas = null;
    }
  }
  if (!canvas) return shape.length * size * 0.62;
  canvas.font = `500 ${size}px "Inter Variable", "Segoe UI Variable", system-ui, sans-serif`;
  return canvas.measureText(shape).width;
}

/** The wing each side needs, clamped to 44…80 (NotchAgentSupport.stripWingRange). */
export function stripWing(reading: string, count: number, stripHeight: number): number {
  const size = stripTextSize(stripHeight);
  const icon = stripIconSize(count, stripHeight);
  const box = { width: 300, height: stripHeight };
  const text = edgeInset(box, size * 0.72, 0) + readingWidth(reading, size);
  const marks = edgeInset(box, icon + 4, (icon + 4) / 2) + marksWidth(count, icon);
  return Math.min(80, Math.max(44, Math.ceil(Math.max(text, marks) + CAMERA_GAP)));
}

/** The one-row content of the capsule: marks, a gap, the reading. */
export function capsuleContent(reading: string, count: number, stripHeight: number): number {
  return Math.ceil(
    marksWidth(count, stripIconSize(count, stripHeight)) + 8 + readingWidth(reading, stripTextSize(stripHeight)),
  );
}

/** What the strip reads now, with the person's choices. */
export function currentReading(snapshot: AgentSnapshot, now: number): string {
  const s = useIsland.getState().settings;
  return stripReading(snapshot, s.agentsReadout, s.agentsLimitDisplay, s.agentsLimitFocus, now);
}

export function AgentsStrip({ geometry, size }: { geometry: Geometry; size: Size }) {
  const snapshot = useAgents((s) => s.snapshot);
  const readout = useIsland((s) => s.settings.agentsReadout);
  useIsland((s) => s.settings.agentsLimitDisplay);
  useIsland((s) => s.settings.agentsLimitFocus);
  const ticking = useClock(advancesWithClock(readout) ? 1000 : 3_600_000);
  const working = workingProviders(snapshot);
  const reading = currentReading(snapshot, ticking);
  const textSize = stripTextSize(geometry.stripHeight);
  const icon = stripIconSize(working.length, geometry.stripHeight);
  const capsule = geometry.floatingGap !== null;
  const iconInset = capsule ? 0 : edgeInset(size, icon + 4, (icon + 4) / 2, 5, geometry.input);
  const textInset = capsule ? 0 : edgeInset(size, textSize * 0.72, 0, 5, geometry.input);
  const wing = capsule ? undefined : (size.width - geometry.cameraWidth) / 2;
  const tint = working[0] ? PROVIDER_TINTS[working[0]] : "#ffffff";
  const label = working.map((p: Provider) => PROVIDER_NAMES[p]).join(", ");

  return (
    <button
      type="button"
      className={`ag-strip${capsule ? " is-capsule" : ""}`}
      style={{ width: size.width, height: size.height, padding: capsule ? "0 14px" : undefined }}
      aria-label={`${label}: ${reading}`}
      title={label}
      onClick={() => useIsland.getState().open("agents")}
    >
      <span className="ag-strip-wing" style={{ width: wing, paddingLeft: iconInset, justifyContent: "flex-start" }}>
        {wing === undefined || wing >= 28
          ? working.map((provider) => <Glyph key={provider} provider={provider} size={icon} />)
          : null}
      </span>
      {capsule ? null : <span style={{ width: geometry.cameraWidth, flex: "none" }} />}
      <span
        className="ag-strip-wing ag-strip-reading"
        style={{ width: wing, paddingRight: textInset, justifyContent: "flex-end", fontSize: textSize, color: tint }}
      >
        {wing === undefined || wing >= 42 ? reading : null}
      </span>
    </button>
  );
}
