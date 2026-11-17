// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Review of 99ff59f7: resolve() checks that quickAccess is a list, but not
// what the list holds, so a hand-edited or older file can still crash the
// island (store.ts expandedHeight, Expanded.tsx, QuickAccess.tsx all read
// b.side / b.action.kind on every entry).

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../island/settings";
import { resolve, type StoredPrefs } from "./prefs";

const stored = (island: Record<string, unknown>) => ({ island }) as unknown as StoredPrefs;

describe("quickAccess entries of the wrong shape", () => {
  it("never reach the island's readers", () => {
    const p = resolve(stored({ quickAccess: [null, 5, { id: "x" }, { id: "y", side: "left", action: null }] }));
    // The same reads store.ts:346 and Expanded.tsx:38 do on every render.
    expect(() => p.island.quickAccess.some((b) => b.side === "bottom")).not.toThrow();
    expect(() => p.island.quickAccess.some((b) => b.action.kind === "explore")).not.toThrow();
  });

  it("keep well-formed buttons", () => {
    const p = resolve(stored({ quickAccess: DEFAULT_SETTINGS.quickAccess.slice(0, 2) }));
    expect(p.island.quickAccess).toEqual(DEFAULT_SETTINGS.quickAccess.slice(0, 2));
  });
});
