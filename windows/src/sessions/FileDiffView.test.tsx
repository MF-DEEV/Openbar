// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { diffNote, FileDiffView, splitPath } from "./FileDiffView";
import { PermissionCard } from "./PermissionCard";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";
import type { FileDiff } from "./types";

vi.mock("./store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./store")>();
  const store = actual.useSessions;
  return {
    ...actual,
    useSessions: Object.assign(
      (select: (state: ReturnType<typeof store.getState>) => unknown) => select(store.getState()),
      store,
    ),
  };
});

beforeEach(() => {
  useSessions.setState({ snapshot: demoSnapshot(), autoAllow: {} });
});

const edit: FileDiff = {
  path: "C:\\Users\\vors\\app\\src\\pay.ts",
  change: "update",
  everyMatch: false,
  added: 1,
  removed: 1,
  hidden: 0,
  lines: [
    { kind: "context", text: "export function pay() {", number: 11 },
    { kind: "removed", text: "  return 1;", number: 12 },
    { kind: "added", text: "  return 2;", number: 12 },
    { kind: "gap", text: "" },
    { kind: "context", text: "}", number: 40 },
  ],
};

it("splits a path at either separator", () => {
  expect(splitPath("C:\\a\\b.ts")).toEqual({ folder: "C:\\a\\", name: "b.ts" });
  expect(splitPath("/w/a/b.ts")).toEqual({ folder: "/w/a/", name: "b.ts" });
  expect(splitPath("b.ts")).toEqual({ folder: "", name: "b.ts" });
});

it("says what happens to the file beyond its lines", () => {
  expect(diffNote(edit)).toBeUndefined();
  expect(diffNote({ ...edit, change: "add" })).toBe("New file");
  expect(diffNote({ ...edit, change: "delete" })).toBe("Deletes the file");
  expect(diffNote({ ...edit, change: "replace" })).toBe("Replaces the whole file");
  expect(diffNote({ ...edit, places: 3 })).toBe("3 places");
  expect(diffNote({ ...edit, everyMatch: true })).toBe("Every match");
  expect(diffNote({ ...edit, cell: "intro" })).toBe("Cell intro");
  expect(diffNote({ ...edit, cell: "" })).toBeUndefined();
  expect(diffNote({ ...edit, cell: "", change: "add" })).toBe("New cell");
  expect(diffNote({ ...edit, cell: "x", change: "delete" })).toBe("Deletes the cell");
  expect(diffNote({ ...edit, movedTo: "/w/b.ts" })).toBe("Moves to /w/b.ts");
});

it("draws removed and added lines with their numbers, counts and a gap", () => {
  const html = renderToStaticMarkup(<FileDiffView diffs={[{ ...edit, hidden: 7 }]} />);
  expect(html).toContain('title="C:\\Users\\vors\\app\\src\\pay.ts"');
  expect(html).toContain("+1");
  expect(html).toContain("−1");
  expect(html).toContain('class="s-diff-line is-removed"');
  expect(html).toContain('class="s-diff-line is-added"');
  expect(html).toContain('class="s-diff-line is-gap"');
  expect(html).toContain('aria-label="Added, line 12:   return 2;"');
  expect(html).toContain("--s-diff-digits:2");
  expect(html).toContain("… 7 more lines");
});

it("leaves the number column out when the file could not be read", () => {
  const html = renderToStaticMarkup(
    <FileDiffView diffs={[{ ...edit, lines: edit.lines.map(({ number: _, ...line }) => line) }]} />,
  );
  expect(html).not.toContain("s-diff-number");
  expect(html).toContain('aria-label="Removed:   return 1;"');
});

it("shows each file under its own name when a change touches several", () => {
  const html = renderToStaticMarkup(<FileDiffView diffs={[edit, { ...edit, path: "/w/other.ts" }]} />);
  expect(html.match(/s-diff-header/g)?.length).toBe(2);
  expect(html.indexOf("pay.ts")).toBeLessThan(html.indexOf("other.ts"));
});

it("puts the diff on the card in place of the bare path, keeping the buttons", () => {
  const base = demoSnapshot().sessions[0]!;
  const session = {
    ...base,
    permission: { ...base.permission!, tool: "Edit", detail: edit.path, diffs: [edit], fitsOnALine: false },
  };
  const html = renderToStaticMarkup(<PermissionCard session={session} onOpen={() => {}} />);
  expect(html).toContain("s-diff-scroll");
  expect(html).not.toContain('class="s-detail"');
  expect(html).toContain('aria-keyshortcuts="Control+y"');
  expect(html).toContain('aria-keyshortcuts="Control+n"');
  const plain = renderToStaticMarkup(<PermissionCard session={base} onOpen={() => {}} />);
  expect(plain).toContain('class="s-detail"');
  expect(plain).not.toContain("s-diff");
});

it("shows an agent's file change above where to answer it", () => {
  const base = demoSnapshot().sessions[0]!;
  const session = { ...base, provider: "gemini" as const, permission: undefined, waitingDiffs: [edit] };
  const html = renderToStaticMarkup(<PermissionCard session={session} onOpen={() => {}} />);
  expect(html).toContain("s-diff-scroll");
  expect(html.indexOf("s-diff-scroll")).toBeLessThan(html.indexOf("s-card-terminal"));
});
