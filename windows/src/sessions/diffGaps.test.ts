// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { diffRows, EXPAND_STEP, openGap, type Opened, type Way } from "./diffGaps";
import type { DiffGap, DiffLine } from "./types";

const ctx = (n: number): DiffLine => ({ kind: "context", text: `line ${n}`, number: n });
const range = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => ctx(from + i));

/** Rows as text: numbers for lines, [v^*N] for a gap row's ways, ~ for a bare gap. */
function shown(lines: DiffLine[], opened: Record<number, Opened> = {}): string {
  return diffRows(lines, opened)
    .map((r) =>
      r.type === "line"
        ? String(r.line.number ?? r.line.text)
        : !r.edge || !(r.down || r.up || r.all)
          ? "~"
          : `[${r.down ? "v" : ""}${r.up ? "^" : ""}${r.all ? "*" : ""}${r.hidden}]`,
    )
    .join(",");
}

function click(lines: DiffLine[], index: number, ways: Way[], opened: Record<number, Opened> = {}) {
  const gap = lines[index]!.gap!;
  const next = { ...opened };
  for (const way of ways) next[index] = openGap(gap, next[index] ?? { down: 0, up: 0 }, way);
  return next;
}

const gapLine = (gap: DiffGap): DiffLine => ({ kind: "gap", text: "", gap });

// As file_diff.rs builds an edit of line 50 in a 100-line file with 8 lines
// kept between two changes at 10 and 80.
const top = gapLine({ edge: "top", count: 46, head: [], tail: range(1, 46) });
const change = (n: number): DiffLine[] => [
  ...range(n - 3, n - 1),
  { kind: "removed", text: `line ${n}`, number: n },
  { kind: "added", text: "changed", number: n },
  ...range(n + 1, n + 3),
];
const middle = gapLine({ edge: "middle", count: 26, head: range(54, 66), tail: range(67, 76) });
const bottom = gapLine({ edge: "bottom", count: 17, head: range(84, 100), tail: [] });
const lines = [top, ...change(50), middle, ...change(80), bottom];

it("shows a row for each gap and opens a long one a click at a time", () => {
  expect(shown(lines)).toBe("[^46],47,48,49,50,50,51,52,53,[v^26],77,78,79,80,80,81,82,83,[*17]");
  const once = click(lines, 0, ["up"]);
  expect(
    shown(lines, once).startsWith(
      `[^26],${range(27, 47)
        .map((l) => l.number)
        .join(",")}`,
    ),
  ).toBe(true);
  const twice = click(lines, 0, ["up"], once);
  expect(shown(lines, twice).startsWith("[*6],7,8,")).toBe(true);
  const done = click(lines, 0, ["all"], twice);
  expect(shown(lines, done).startsWith("1,2,3,")).toBe(true);
  // A gap above the first change never opens down.
  expect(click(lines, 0, ["down"])[0]).toEqual({ down: 0, up: 0 });
});

it("opens a gap between changes from either side, then whole, with numbers in order", () => {
  const index = lines.indexOf(middle);
  const down = click(lines, index, ["down"]);
  // 13 kept from the start: one click shows them all, 13 left, 10 kept.
  expect(shown(lines, down)).toContain(",53,54,55,56,57,58,59,60,61,62,63,64,65,66,[^13],77,");
  const up = click(lines, index, ["up"], down);
  expect(shown(lines, up)).toContain(",66,~,67,");
  const all = click(lines, index, ["all"]);
  const numbers = diffRows(lines, all).flatMap((r) => (r.type === "line" ? [r.line.number!] : []));
  const run = numbers.slice(numbers.indexOf(47), numbers.lastIndexOf(83) + 1).filter((n, i, a) => a[i - 1] !== n);
  expect(run).toEqual(range(47, 83).map((l) => l.number));
  expect(shown(lines, all)).toContain(",53,54,");
  expect(shown(lines, all)).not.toContain("[v^");
});

it("keeps a key per row that stays put while gaps open", () => {
  const rows = diffRows(lines, click(lines, 0, ["up", "up", "all"]));
  expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  const before = diffRows(lines, click(lines, 0, ["up"])).filter((r) => r.type === "line");
  const after = diffRows(lines, click(lines, 0, ["up", "up"])).filter((r) => r.type === "line");
  const keyOf = (rows: typeof before, n: number) =>
    rows.find((r) => r.type === "line" && r.line.number === n && r.line.kind === "context")?.key;
  expect(keyOf(before, 30)).toBe(keyOf(after, 30));
});

it("shows nothing to open where nothing is kept", () => {
  const bare: DiffLine[] = [ctx(1), { kind: "gap", text: "" }, ctx(40)];
  expect(shown(bare)).toBe("1,~,40");
  const empty = gapLine({ edge: "top", count: 30, head: [], tail: [] });
  expect(shown([empty, ...change(40)])).toBe("37,38,39,40,40,41,42,43");
  const spent = gapLine({ edge: "middle", count: 30, head: [], tail: [] });
  expect(shown([...change(5), spent, ...change(50)])).toContain(",8,~,47,");
  // Past what is kept, a top gap stays as a plain mark.
  const partial = gapLine({ edge: "top", count: 60, head: [], tail: range(41, 46) });
  const open = click([partial, ...change(50)], 0, ["up"]);
  expect(shown([partial, ...change(50)], open).startsWith("~,41,")).toBe(true);
  expect(EXPAND_STEP).toBe(20);
});
