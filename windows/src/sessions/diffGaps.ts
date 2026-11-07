// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Opening the gaps of a file change's diff (ClaudeFileDiffs.rows on the Mac):
// the lines come with the diff, so a click reads nothing.

import type { DiffGap, DiffLine } from "./types";

/** Lines one click on a gap shows (EXPAND_STEP in file_diff.rs). */
export const EXPAND_STEP = 20;

/** How far one gap is open: lines shown from its start and from its end. */
export interface Opened {
  down: number;
  up: number;
}

/** down: below the change above; up: above the change below; all: the rest. */
export type Way = "down" | "up" | "all";

export type DiffRow =
  | { type: "line"; line: DiffLine; key: string }
  | {
      type: "gap";
      key: string;
      /** The gap's place in the diff's lines. */
      index: number;
      edge?: DiffGap["edge"];
      hidden: number;
      down: boolean;
      up: boolean;
      all: boolean;
    };

/** Never more than a gap keeps, nor lines shown twice. */
function clamped(opened: Opened, gap: DiffGap): Opened {
  const down = Math.min(Math.max(0, opened.down), gap.head.length);
  const up = Math.min(Math.max(0, opened.up), gap.tail.length, gap.count - down);
  return { down, up };
}

/** The diff's lines with each gap opened as far as `opened` says. */
export function diffRows(lines: DiffLine[], opened: Record<number, Opened>): DiffRow[] {
  const rows: DiffRow[] = [];
  lines.forEach((line, index) => {
    if (line.kind !== "gap") {
      rows.push({ type: "line", line, key: `${index}` });
      return;
    }
    const gap = line.gap;
    if (!gap) {
      rows.push({ type: "gap", key: `gap${index}`, index, hidden: 0, down: false, up: false, all: false });
      return;
    }
    const open = clamped(opened[index] ?? { down: 0, up: 0 }, gap);
    gap.head.slice(0, open.down).forEach((kept, k) => rows.push({ type: "line", line: kept, key: `${index}h${k}` }));
    const hidden = gap.count - open.down - open.up;
    if (hidden > 0) {
      const down = gap.head.length > open.down;
      const up = gap.tail.length > open.up;
      // The rest fits one click and is all kept: one way shows it.
      const all = hidden <= EXPAND_STEP && gap.head.length - open.down + gap.tail.length - open.up === hidden;
      // Nothing kept past the file's first or last change: no row, as
      // before the card could open gaps.
      if (gap.edge === "middle" || gap.head.length > 0 || gap.tail.length > 0) {
        rows.push({
          type: "gap",
          key: `gap${index}`,
          index,
          edge: gap.edge,
          hidden,
          down: down && !all,
          up: up && !all,
          all,
        });
      }
    }
    const first = gap.tail.length - open.up;
    gap.tail.slice(first).forEach((kept, k) => rows.push({ type: "line", line: kept, key: `${index}t${first + k}` }));
  });
  return rows;
}

/** `opened` one click further the given way. */
export function openGap(gap: DiffGap, opened: Opened, way: Way): Opened {
  const next = clamped(opened, gap);
  if (way === "down") next.down += EXPAND_STEP;
  else if (way === "up") next.up += EXPAND_STEP;
  else return clamped({ down: gap.head.length, up: gap.tail.length }, gap);
  return clamped(next, gap);
}
