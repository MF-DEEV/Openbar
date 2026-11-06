// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's own git worktree, as the page reads it (SessionWorktrees.swift,
// worktree.rs): the name a prompt gives one, and which worktree a session
// works in.

/** Folders agents keep worktrees in, inside the repository they belong to. */
const MARKERS = ["/.claude/worktrees/", "/.openbar/worktrees/", "/.gemini/worktrees/"];

/** Letters that don't decompose into an ASCII one and a mark. */
const OWN_LETTERS: Record<string, string> = {
  ł: "l",
  Ł: "L",
  ø: "o",
  Ø: "O",
  ß: "ss",
  æ: "ae",
  Æ: "AE",
  œ: "oe",
  Œ: "OE",
  đ: "d",
  Đ: "D",
  ð: "d",
  Ð: "D",
  þ: "th",
  Þ: "TH",
  ı: "i",
};

/** A name from the first prompt: `Fix the login bug!` as `fix-the-login-bug`. */
export function worktreeSlug(text: string, limit = 40): string {
  let folded = "";
  for (const c of text.normalize("NFD")) {
    if (c.charCodeAt(0) < 128) folded += c;
    else if (OWN_LETTERS[c]) folded += OWN_LETTERS[c];
    else if (!/\p{Mn}/u.test(c)) folded += " ";
  }
  let slug = "";
  for (const word of folded.split(/[^A-Za-z0-9]+/).filter(Boolean)) {
    const next = slug ? `${slug}-${word}` : word;
    if (next.length > limit) {
      if (!slug) slug = word.slice(0, limit);
      break;
    }
    slug = next;
  }
  return slug.toLowerCase();
}

function markerEnd(cwd: string): number | null {
  const normal = cwd.replace(/\\/g, "/").toLowerCase();
  for (const marker of MARKERS) {
    const at = normal.indexOf(marker);
    if (at >= 0) return at + marker.length;
  }
  return null;
}

/** The worktree a session works in, when an agent or the island made it one. */
export function worktreeName(cwd: string | undefined): string | null {
  if (!cwd) return null;
  const end = markerEnd(cwd);
  if (end === null) return null;
  return cwd.slice(end).split(/[\\/]/)[0] || null;
}

/** Whether the island made it, so the island may remove it. */
export function isOwnWorktree(cwd: string | undefined): boolean {
  return !!cwd && cwd.replace(/\\/g, "/").toLowerCase().includes("/.openbar/worktrees/");
}
