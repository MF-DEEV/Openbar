// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { makeClock } from "./clock";

/** A webview clock and Windows' interrupt time, moved by hand. */
function fixture() {
  const t = { perf: 1000, wall: 1_700_000_000_000, ticks: 50_000_000 as number | undefined };
  const clock = makeClock({ perf: () => t.perf, wall: () => t.wall, ticks: async () => t.ticks });
  const advance = (ms: number, counted = true) => {
    if (counted) t.perf += ms;
    t.wall += ms;
    if (t.ticks !== undefined) t.ticks += ms;
  };
  return { t, clock, advance };
}

describe("continuous clock", () => {
  it("keeps performance.now()'s frame when the first reading only sets the base", async () => {
    const { clock } = fixture();
    expect(clock.now()).toBe(1);
    await clock.resync();
    expect(clock.now()).toBe(1);
  });

  it("adds a sleep that performance.now() did not count", async () => {
    const { clock, advance } = fixture();
    await clock.resync();
    advance(10_000);
    advance(3_600_000, false);
    expect(clock.jumped()).toBe(true);
    expect(clock.jumped()).toBe(false);
    await clock.resync();
    expect(clock.now()).toBeCloseTo(1 + 10 + 3600);
  });

  it("changes nothing when performance.now() counted the sleep too", async () => {
    const { clock, advance } = fixture();
    await clock.resync();
    advance(3_600_000);
    expect(clock.jumped()).toBe(false);
    await clock.resync();
    expect(clock.now()).toBeCloseTo(3601);
  });

  it("ignores jitter and never runs backwards", async () => {
    const { t, clock } = fixture();
    await clock.resync();
    t.ticks! += 100;
    await clock.resync();
    expect(clock.now()).toBe(1);
    t.ticks! -= 5000;
    await clock.resync();
    expect(clock.now()).toBe(1);
  });

  it("runs on performance.now() alone outside the app", async () => {
    const { t, clock, advance } = fixture();
    t.ticks = undefined;
    await clock.resync();
    advance(2000);
    await clock.resync();
    expect(clock.now()).toBe(3);
  });
});
