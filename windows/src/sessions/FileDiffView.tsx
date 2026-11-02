// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What a file change would do, file by file (NotchClaudeFileDiffView.swift):
// removed lines in red, added ones in green, a few unchanged ones around
// them. Long diffs scroll; the service already cut very large ones.

import { FileMinus, FilePlus, FileText } from "lucide-react";
import { useLocale } from "../i18n";
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
          <span className="s-diff-folder">{folder}</span>
          <span className="s-diff-name">{name}</span>
        </span>
        <span className="s-diff-counts" aria-hidden>
          <span className="is-added">+{diff.added}</span>
          <span className="is-removed">−{diff.removed}</span>
        </span>
      </div>
      {note ? <div className="s-diff-note">{note}</div> : null}
    </div>
  );
}

const SIGN: Record<DiffLine["kind"], string> = { added: "+", removed: "−", context: " ", gap: "" };

function Lines({ diff }: { diff: FileDiff }) {
  const digits = String(Math.max(0, ...diff.lines.map((l) => l.number ?? 0))).length;
  const numbered = diff.lines.some((l) => l.number !== undefined);
  return (
    <div className="s-diff-lines" style={{ ["--s-diff-digits" as string]: numbered ? digits : 0 }}>
      {diff.lines.map((line, i) =>
        line.kind === "gap" ? (
          <div key={i} className="s-diff-line is-gap" aria-hidden>
            ⋯
          </div>
        ) : (
          <div key={i} className={`s-diff-line is-${line.kind}`} aria-label={S.diffLineLabel(line)}>
            {numbered ? <span className="s-diff-number">{line.number ?? ""}</span> : null}
            <span className="s-diff-sign" aria-hidden>
              {SIGN[line.kind]}
            </span>
            <span className="s-diff-text">{line.text || " "}</span>
          </div>
        ),
      )}
      {diff.hidden > 0 ? <div className="s-diff-more">{S.diffMoreLines(diff.hidden)}</div> : null}
    </div>
  );
}

export function FileDiffView({ diffs }: { diffs: FileDiff[] }) {
  useLocale();
  const one = diffs.length === 1;
  return (
    <div className="s-diff">
      {one ? <Header diff={diffs[0]!} /> : null}
      <div className="s-diff-scroll" tabIndex={0} role="group" aria-label={S.diffLabel}>
        {diffs.map((diff, i) => (
          <div key={i} className="s-diff-block">
            {one ? null : <Header diff={diff} />}
            <Lines diff={diff} />
          </div>
        ))}
      </div>
    </div>
  );
}
