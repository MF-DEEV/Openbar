// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What a file change would do, file by file (NotchClaudeFileDiffView.swift):
// removed lines in red, added ones in green, a few unchanged ones around
// them, and a row to show more of the file where lines are left out. Long
// diffs scroll; the service already cut very large ones.

import { ArrowDown, ArrowUp, ArrowUpDown, FileMinus, FilePlus, FileText } from "lucide-react";
import { useState } from "react";
import { useLocale } from "../i18n";
import { diffRows, openGap, type DiffRow, type Opened, type Way } from "./diffGaps";
import { S } from "./strings";
import type { DiffLine, FileDiff } from "./types";

/** The folder with its trailing separator, and the file's name. */
export function splitPath(path: string): { folder: string; name: string } {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut < 0 ? { folder: "", name: path } : { folder: path.slice(0, cut + 1), name: path.slice(cut + 1) };
}

/** What happens to the file, beyond its lines: new, deleted, how many places. */
export function diffNote(diff: FileDiff): string | undefined {
  const parts: string[] = [];
  const cell = diff.cell !== undefined;
  if (diff.change === "add") parts.push(cell ? S.diffNewCell : S.diffNewFile);
  else if (diff.change === "delete") parts.push(cell ? S.diffDeletedCell : S.diffDeletedFile);
  else if (diff.change === "replace") parts.push(S.diffReplacesFile);
  else if (diff.cell) parts.push(S.diffCell(diff.cell));
  if (diff.places) parts.push(S.diffPlaces(diff.places));
  if (diff.everyMatch) parts.push(S.diffEveryMatch);
  if (diff.movedTo) parts.push(S.diffMovesTo(diff.movedTo));
  return parts.length ? parts.join(" · ") : undefined;
}

function Header({ diff }: { diff: FileDiff }) {
  const { folder, name } = splitPath(diff.path);
  const note = diffNote(diff);
  const Icon = diff.change === "add" ? FilePlus : diff.change === "delete" ? FileMinus : FileText;
  return (
    <div
      className="s-diff-header"
      aria-label={[diff.path, note, S.diffCounts(diff.added, diff.removed)].filter(Boolean).join(", ")}
    >
      <div className="s-diff-file">
        <Icon size={11} strokeWidth={2.4} aria-hidden />
        <span className="s-diff-path" title={diff.path}>
          {/* The folder gives way first, cut from its start; the name stays whole. */}
          <span className="s-diff-folder">
            <bdi>{folder}</bdi>
          </span>
          <span className="s-diff-name">{name}</span>
        </span>
        {note ? <span className="s-diff-note">{note}</span> : null}
        <span className="s-diff-counts" aria-hidden>
          <span className="is-added">+{diff.added}</span>
          <span className="is-removed">−{diff.removed}</span>
        </span>
      </div>
    </div>
  );
}

const SIGN: Record<DiffLine["kind"], string> = { added: "+", removed: "−", context: " ", gap: "" };

/** A row in place of lines a diff leaves out: arrows that show the next
 *  lines from either side, or all of them when few are left. */
function GapRow({ row, onOpen }: { row: Extract<DiffRow, { type: "gap" }>; onOpen: (way: Way) => void }) {
  if (!row.edge || !(row.down || row.up || row.all)) {
    return (
      <div className="s-diff-line is-gap" aria-hidden>
        ⋯
      </div>
    );
  }
  const AllIcon = row.edge === "top" ? ArrowUp : row.edge === "bottom" ? ArrowDown : ArrowUpDown;
  const arrow = (way: Way, Icon: typeof ArrowUp, label: string) => (
    <button type="button" className="s-diff-expand" title={label} aria-label={label} onClick={() => onOpen(way)}>
      <Icon size={10} strokeWidth={2.6} aria-hidden />
    </button>
  );
  return (
    <div className="s-diff-line is-expand">
      {row.all ? arrow("all", AllIcon, S.diffShowAll(row.hidden)) : null}
      {row.down ? arrow("down", ArrowDown, S.diffShowBelow) : null}
      {row.up ? arrow("up", ArrowUp, S.diffShowAbove) : null}
      <span className="s-diff-hidden" aria-hidden>
        {S.diffHiddenLines(row.hidden)}
      </span>
    </div>
  );
}

function Lines({
  diff,
  opened,
  onOpened,
}: {
  diff: FileDiff;
  opened: Record<number, Opened>;
  onOpened: (opened: Record<number, Opened>) => void;
}) {
  const rows = diffRows(diff.lines, opened);
  const shown = rows.flatMap((r) => (r.type === "line" && r.line.number !== undefined ? [r.line.number] : []));
  const digits = String(Math.max(0, ...shown)).length;
  const numbered = diff.lines.some((l) => l.number !== undefined);
  return (
    <div className="s-diff-lines" style={{ ["--s-diff-digits" as string]: numbered ? digits : 0 }}>
      {rows.map((row) =>
        row.type === "gap" ? (
          <GapRow
            key={row.key}
            row={row}
            onOpen={(way) => {
              const gap = diff.lines[row.index]?.gap;
              if (gap) onOpened({ ...opened, [row.index]: openGap(gap, opened[row.index] ?? { down: 0, up: 0 }, way) });
            }}
          />
        ) : (
          <div
            key={row.key}
            // Lines a click brings in fade in where they land.
            className={`s-diff-line is-${row.line.kind}${/\D/.test(row.key) ? " is-revealed" : ""}`}
            aria-label={S.diffLineLabel(row.line)}
          >
            {numbered ? <span className="s-diff-number">{row.line.number ?? ""}</span> : null}
            <span className="s-diff-sign" aria-hidden>
              {SIGN[row.line.kind]}
            </span>
            <span className="s-diff-text">{row.line.text || " "}</span>
          </div>
        ),
      )}
      {diff.hidden > 0 ? <div className="s-diff-more">{S.diffMoreLines(diff.hidden)}</div> : null}
    </div>
  );
}

/** What the diffs are, to tell a new request's from a fresh copy of the same. */
function signature(diffs: FileDiff[]): string {
  return diffs
    .map((d) => [d.path, d.change, d.added, d.removed, d.hidden, d.lines.length, d.lines[0]?.number].join(":"))
    .join("|");
}

export function FileDiffView({ diffs }: { diffs: FileDiff[] }) {
  useLocale();
  // How far each gap is open, by file and by the gap's place in its lines.
  // Other diffs start closed; a fresh snapshot of the same ones keeps them.
  const sig = signature(diffs);
  const [opened, setOpened] = useState<{ sig: string; files: Record<number, Record<number, Opened>> }>({
    sig,
    files: {},
  });
  const files = opened.sig === sig ? opened.files : {};
  const one = diffs.length === 1;
  return (
    <div className="s-diff">
      {one ? <Header diff={diffs[0]!} /> : null}
      <div className="s-diff-scroll" tabIndex={0} role="group" aria-label={S.diffLabel}>
        {diffs.map((diff, i) => (
          <div key={i} className="s-diff-block">
            {one ? null : <Header diff={diff} />}
            <Lines
              diff={diff}
              opened={files[i] ?? {}}
              onOpened={(next) => setOpened({ sig, files: { ...files, [i]: next } })}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
