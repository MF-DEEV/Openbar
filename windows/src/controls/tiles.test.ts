// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { registerTileReading, useTileRegistry } from "./tiles";

describe("tile readings", () => {
  it("start empty and let the newest registration win", () => {
    expect(useTileRegistry.getState().hooks.timer).toBeUndefined();
    const first = () => ({ reading: "1:00" });
    const second = () => ({ reading: "2:00" });
    registerTileReading("timer", first);
    const a = useTileRegistry.getState().hooks.timer!;
    registerTileReading("timer", second);
    const b = useTileRegistry.getState().hooks.timer!;
    expect(b.hook).toBe(second);
    expect(b.id).toBeGreaterThan(a.id);
    expect(useTileRegistry.getState().hooks.calendar).toBeUndefined();
  });
});
