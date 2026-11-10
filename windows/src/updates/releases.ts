// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Which of the project's GitHub releases is newer than the running build. The
// Windows app's releases are tagged `windows-v<version>`; every other tag
// belongs to the Mac app or to a demo, and drafts are not public.

export const FEED_URL = "https://api.github.com/repos/MF-DEEV/Openbar/releases?per_page=30";
const PAGE = "https://github.com/MF-DEEV/Openbar/releases/tag/";
const PREFIX = "windows-v";

export interface Release {
  version: string;
  /** The release's own page, where its installer and notes are. */
  url: string;
}

interface Parsed {
  core: [number, number, number];
  /** The dot-separated parts after the hyphen ("beta.2"); none for a final release. */
  pre: string[];
}

const VERSION = /^(\d{1,9})\.(\d{1,9})\.(\d{1,9})(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

export function parseVersion(text: string): Parsed | null {
  const found = VERSION.exec(text.trim());
  if (!found) return null;
  return { core: [Number(found[1]), Number(found[2]), Number(found[3])], pre: found[4] ? found[4].split(".") : [] };
}

const numeric = (part: string) => /^\d+$/.test(part);

function comparePre(a: string[], b: string[]): number {
  // A final release comes after its own pre-releases.
  if (!a.length || !b.length) return a.length === b.length ? 0 : a.length ? -1 : 1;
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    const [x, y] = [a[i]!, b[i]!];
    if (numeric(x) && numeric(y)) {
      if (Number(x) !== Number(y)) return Number(x) < Number(y) ? -1 : 1;
    } else if (numeric(x) !== numeric(y)) {
      // Numbers come before words.
      return numeric(x) ? -1 : 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return a.length === b.length ? 0 : a.length < b.length ? -1 : 1;
}

/** Semantic Versioning's precedence: negative when `a` is older than `b`. */
function compare(a: Parsed, b: Parsed): number {
  for (let i = 0; i < 3; i++) {
    if (a.core[i] !== b.core[i]) return a.core[i]! < b.core[i]! ? -1 : 1;
  }
  return comparePre(a.pre, b.pre);
}

/**
 * The newest release for this app that is newer than `current`, or null. A
 * pre-release (a version with a hyphen) is offered only to a build that is
 * one too; anything the feed gets wrong is skipped.
 */
export function newestRelease(feed: unknown, current: string): Release | null {
  const running = parseVersion(current);
  if (!running || !Array.isArray(feed)) return null;
  let best: { parsed: Parsed; release: Release } | null = null;
  for (const entry of feed as unknown[]) {
    if (!entry || typeof entry !== "object") continue;
    const { tag_name: tag, draft } = entry as Record<string, unknown>;
    if (draft === true || typeof tag !== "string" || !tag.startsWith(PREFIX)) continue;
    const version = tag.slice(PREFIX.length);
    const parsed = parseVersion(version);
    if (!parsed || (parsed.pre.length && !running.pre.length)) continue;
    if (compare(parsed, running) <= 0) continue;
    if (!best || compare(parsed, best.parsed) > 0) best = { parsed, release: { version, url: PAGE + tag } };
  }
  return best?.release ?? null;
}
