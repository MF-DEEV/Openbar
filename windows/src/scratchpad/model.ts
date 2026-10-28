// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The scratchpad's document rules (ScratchpadDocument / ScratchpadSupport,
// ScratchpadSupport.swift:57-160, 502-541), pure for the tests.

import type { ScratchpadRetention } from "../island/settings";
import type { Pad, ScratchpadDocument } from "./types";

export const MAX_PADS = 12;
export const MAX_NAME = 40;
export const TEXT_SIZE = { min: 10, max: 22, default: 13 } as const;

export const newId = (): string => crypto.randomUUID();

/** Words joined by single spaces, at most 40 characters. */
export const sanitizedName = (name: string) =>
  [...name.split(/\s+/).filter(Boolean).join(" ")].slice(0, MAX_NAME).join("");

/** "Scratchpad 1", or the first free number after it. */
export function nextPadName(base: string, existing: readonly string[]): string {
  const safe = sanitizedName(base) || "Scratchpad";
  const used = new Set(existing);
  if (!used.has(safe) && !used.has(`${safe} 1`)) return `${safe} 1`;
  for (let n = 2; n <= MAX_PADS; n++) if (!used.has(`${safe} ${n}`)) return `${safe} ${n}`;
  return `${safe} ${existing.length + 1}`;
}

export function initialDocument(base: string, id = newId()): ScratchpadDocument {
  return { pads: [{ id, name: nextPadName(base, []), text: "" }], selectedId: id };
}

export const selectedPad = (doc: ScratchpadDocument): Pad | undefined => doc.pads.find((p) => p.id === doc.selectedId);

export function addPad(doc: ScratchpadDocument, base: string, id = newId()): ScratchpadDocument | null {
  if (doc.pads.length >= MAX_PADS) return null;
  const name = nextPadName(
    base,
    doc.pads.map((p) => p.name),
  );
  return { pads: [...doc.pads, { id, name, text: "" }], selectedId: id };
}

export function renamePad(doc: ScratchpadDocument, id: string, proposed: string): ScratchpadDocument | null {
  const name = sanitizedName(proposed);
  if (!name || !doc.pads.some((p) => p.id === id)) return null;
  return { ...doc, pads: doc.pads.map((p) => (p.id === id ? { ...p, name } : p)) };
}

/** The last pad stays; closing the chosen one chooses its neighbour. */
export function removePad(doc: ScratchpadDocument, id: string): ScratchpadDocument | null {
  const index = doc.pads.findIndex((p) => p.id === id);
  if (doc.pads.length <= 1 || index < 0) return null;
  const pads = doc.pads.filter((p) => p.id !== id);
  const selectedId = doc.selectedId === id ? pads[Math.min(index, pads.length - 1)]!.id : doc.selectedId;
  return { pads, selectedId };
}

/** An empty pad closes without asking (ScratchpadSupport.requiresCloseConfirmation). */
export const closeNeedsConfirmation = (pad: Pad) => pad.text.length > 0;

export function withSelectedText(doc: ScratchpadDocument, text: string, now: number): ScratchpadDocument {
  return {
    ...doc,
    pads: doc.pads.map((p) =>
      p.id === doc.selectedId && p.text !== text ? { ...p, text, modifiedAt: text ? now : undefined } : p,
    ),
  };
}

/** Tabs in their new order (a pointer drag in the tab strip). */
export function reorderPads(doc: ScratchpadDocument, ids: readonly string[]): ScratchpadDocument {
  const byId = new Map(doc.pads.map((p) => [p.id, p]));
  const pads = ids.map((id) => byId.get(id)).filter((p): p is Pad => !!p);
  return pads.length === doc.pads.length ? { ...doc, pads } : doc;
}

const RETENTION_MS: Record<ScratchpadRetention, number | null> = {
  never: null,
  day: 86_400_000,
  week: 7 * 86_400_000,
  month: 30 * 86_400_000,
};

/** Text left unedited past the chosen period clears when the pad opens; no date never clears. */
export function applyRetention(
  doc: ScratchpadDocument,
  retention: ScratchpadRetention,
  now: number,
): ScratchpadDocument {
  const limit = RETENTION_MS[retention];
  if (limit === null) return doc;
  const expired = (p: Pad) => p.modifiedAt !== undefined && now - p.modifiedAt > limit;
  if (!doc.pads.some(expired)) return doc;
  return { ...doc, pads: doc.pads.map((p) => (expired(p) ? { id: p.id, name: p.name, text: "" } : p)) };
}

/** "Scratchpad 1 2026-10-27.txt", path characters made safe. */
export function exportFileName(title: string, date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  const day = `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`;
  return `${sanitizedName(title).replace(/[\\/:*?"<>|]/g, "-")} ${day}.txt`;
}

export const sanitizedTextSize = (size: number) =>
  Number.isFinite(size) ? Math.min(TEXT_SIZE.max, Math.max(TEXT_SIZE.min, Math.round(size))) : TEXT_SIZE.default;

/**
 * Return inside a list item continues the list (PlainTextEditor
 * continuesLists): the same marker, the next number, an empty box for a
 * task. Return at the end of an empty item ends the list instead. Null when
 * the line isn't a list item or the caret sits inside its marker; `insert`
 * replaces `[from, to)`.
 */
export function listContinuation(text: string, caret: number): { from: number; to: number; insert: string } | null {
  const lineStart = text.lastIndexOf("\n", caret - 1) + 1;
  const newline = text.indexOf("\n", caret);
  const line = text.slice(lineStart, newline < 0 ? text.length : newline);
  const match = /^(\s*)([-*+]|\d+[.)])(\s+)(\[[ xX]\]\s+)?/.exec(line);
  if (!match) return null;
  const [marker, indent, bullet, space, box] = match as unknown as [string, string, string, string, string?];
  if (line.trim() === marker.trim()) {
    return caret === lineStart + line.length ? { from: lineStart, to: caret, insert: "" } : null;
  }
  if (caret <= lineStart + marker.length) return null;
  const number = /^\d+/.exec(bullet);
  const nextBullet = number ? `${Number(number[0]) + 1}${bullet.slice(number[0].length)}` : bullet;
  return { from: caret, to: caret, insert: `\n${indent}${nextBullet}${space}${box ? "[ ] " : ""}` };
}

/** Flips the task box whose `[` sits at or after `from` (a click in the preview). */
export function toggleTaskAt(text: string, from: number): string {
  const match = /\[([ xX])\]/.exec(text.slice(from));
  if (!match) return text;
  const at = from + match.index + 1;
  return text.slice(0, at) + (match[1] === " " ? "x" : " ") + text.slice(at + 1);
}
