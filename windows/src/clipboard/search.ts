// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Searching the history (ClipboardHistorySearch, CHSup:664-772): every word
// of the query must appear, ignoring case, accents and character width; the
// results rank whole and leading matches first, pins a little higher, and
// otherwise keep the history's order. The matched parts of a row are drawn
// bold (SearchHighlightText, CHSup:777-816).

import { C } from "./strings";
import type { ClipboardEntry } from "./types";

/** One character folded the way the search compares it. */
function foldChar(c: string): string {
  return c.normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase();
}

/**
 * Case, accent and width folded, with line breaks and tabs as spaces, and
 * trimmed. No locale: a Turkish one would fold a dotted I to a dotless one.
 */
export function normalized(value: string): string {
  return Array.from(value, foldChar)
    .join("")
    .replace(/[\n\r\t]/g, " ")
    .trim();
}

/** The query's words as typed, for highlighting. */
export function searchTokens(query: string): string[] {
  return query.split(/\s+/).filter(Boolean);
}

function words(text: string): Set<string> {
  return new Set(text.split(/\s+/).filter(Boolean));
}

const IMAGE_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "heic",
  "heif",
  "tiff",
  "tif",
  "gif",
  "webp",
  "bmp",
  "ico",
  "icns",
  "svg",
  "avif",
]);

export function isImageFile(path: string): boolean {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 && IMAGE_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function dimensions(entry: Pick<ClipboardEntry, "imageWidth" | "imageHeight">): string {
  return entry.imageWidth && entry.imageHeight ? `${entry.imageWidth}×${entry.imageHeight}` : "";
}

/** What the search can match: an image reads as "Image png W×H", files as their names. */
export function searchableText(entry: ClipboardEntry): string {
  switch (entry.kind) {
    case "text":
      return entry.text;
    case "image":
      return `${C.imageEntryLabel} png ${dimensions(entry)}`;
    case "files": {
      const names = entry.filePaths.map(fileName).join(" ");
      return entry.filePaths.some(isImageFile) ? `${C.imageEntryLabel} ${names}` : names;
    }
  }
}

function score(text: string, textWords: Set<string>, query: string, tokens: string[], pinned: boolean): number {
  let total = pinned ? 30 : 0;
  if (text === query) total += 1200;
  if (text.startsWith(query)) total += 900;
  if (text.includes(query)) total += 700;
  for (const token of tokens) {
    if (textWords.has(token)) total += 140;
    else if ([...textWords].some((w) => w.startsWith(token))) total += 80;
    else total += 40;
  }
  return total;
}

/** The entries that match `query`, best first; all of them, in order, for an empty one. */
/** Each entry folded once: a snapshot's entries stay the same objects while the person types. */
const foldedEntries = new WeakMap<ClipboardEntry, { text: string; words: Set<string> }>();

function folded(entry: ClipboardEntry) {
  let hit = foldedEntries.get(entry);
  if (!hit) {
    const text = normalized(searchableText(entry));
    hit = { text, words: words(text) };
    foldedEntries.set(entry, hit);
  }
  return hit;
}

export function filterEntries<T extends ClipboardEntry>(entries: T[], query: string): T[] {
  const foldedQuery = normalized(query);
  const tokens = foldedQuery.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return entries;
  return entries
    .map((entry, index) => {
      const { text, words: textWords } = folded(entry);
      if (!tokens.every((t) => text.includes(t))) return null;
      return { entry, index, score: score(text, textWords, foldedQuery, tokens, entry.pinned) };
    })
    .filter((m): m is { entry: T; index: number; score: number } => m !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((m) => m.entry);
}

/** More than any row shows within its line limit; past this a row is searched and styled no further. */
const VISIBLE_CHARACTERS = 500;

export interface Segment {
  text: string;
  match: boolean;
}

/**
 * `text` cut to what a row can show, split into plain and matched runs.
 * Matching folds case, accents and width; the runs keep the original text.
 */
export function highlight(text: string, tokens: string[]): Segment[] {
  if (tokens.length === 0 || !text) return [{ text, match: false }];
  const chars = Array.from(text);
  const visible = chars.length > VISIBLE_CHARACTERS ? [...chars.slice(0, VISIBLE_CHARACTERS), "…"] : chars;
  // The folded text, and for each of its code units the character it came from.
  let folded = "";
  const origin: number[] = [];
  visible.forEach((c, i) => {
    const f = foldChar(c);
    folded += f;
    for (let k = 0; k < f.length; k++) origin.push(i);
  });
  const marked = new Array<boolean>(visible.length).fill(false);
  for (const token of tokens) {
    const needle = Array.from(token, foldChar).join("");
    if (!needle) continue;
    for (let at = folded.indexOf(needle); at >= 0; at = folded.indexOf(needle, at + needle.length)) {
      for (let k = at; k < at + needle.length; k++) marked[origin[k]!] = true;
    }
  }
  const segments: Segment[] = [];
  visible.forEach((c, i) => {
    const last = segments[segments.length - 1];
    if (last && last.match === marked[i]) last.text += c;
    else segments.push({ text: c, match: marked[i]! });
  });
  return segments;
}
