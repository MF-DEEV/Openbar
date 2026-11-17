// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Edge cases of the settings window's optimistic writes: a slider sends one
// prefs_set per step, and each answer carries the state after *that* step.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { resolve, type Prefs, type PrefsPatch } from "../shared/prefs";

type Pending = { patch: PrefsPatch; ok: (p: Prefs) => void; fail: (e: unknown) => void };
const pending: Pending[] = [];
let saved = resolve(null);

vi.mock("../shared/prefs", async (importOriginal) => {
  const real = await importOriginal<typeof import("../shared/prefs")>();
  return {
    ...real,
    setPrefs: (patch: PrefsPatch) =>
      new Promise<Prefs>((ok, fail) => {
        pending.push({ patch, ok, fail });
      }),
  };
});

const { useSettings, resetSettings } = await import("./state");

/** Answers the oldest request the way prefs_set would: the saved state after it. */
function answer() {
  const p = pending.shift()!;
  saved = { ...saved, island: { ...saved.island, ...(p.patch.island as Partial<Prefs["island"]>) } };
  p.ok(saved);
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("settings state under rapid patches", () => {
  beforeEach(() => {
    pending.length = 0;
    saved = resolve(null);
    resetSettings();
  });

  it("an older answer does not pull the slider back under the pointer", async () => {
    const s = useSettings.getState();
    s.island("customWidth", 450);
    s.island("customWidth", 460);
    s.island("customWidth", 470);
    expect(useSettings.getState().prefs.island.customWidth).toBe(470);
    answer(); // the answer to 450 arrives while the person is at 470
    await flush();
    expect(useSettings.getState().prefs.island.customWidth).toBe(470);
  });

  it("when every write fails, what is shown is what is saved", async () => {
    const s = useSettings.getState();
    s.island("customWidth", 450);
    s.island("customWidth", 460);
    pending.shift()!.fail("read-only");
    pending.shift()!.fail("read-only");
    await flush();
    expect(useSettings.getState().prefs.island.customWidth).toBe(saved.island.customWidth);
  });
});
