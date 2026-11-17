// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CLOSE_MS, Dwell, DWELL_MS, KEY_DWELL_MS } from "./dwell";

let dwell: Dwell;
let changes = 0;

beforeEach(() => {
  vi.useFakeTimers();
  changes = 0;
  dwell = new Dwell(() => changes++);
});
afterEach(() => {
  dwell.dispose();
  vi.useRealTimers();
});

describe("compact open-on-dwell", () => {
  it("opens after the pointer rests 0.7 s, not before", () => {
    dwell.enter("a");
    expect(dwell.hovered).toBe("a");
    vi.advanceTimersByTime(DWELL_MS - 1);
    expect(dwell.opened).toBeNull();
    vi.advanceTimersByTime(1);
    expect(dwell.opened).toBe("a");
  });

  it("passing over a row doesn't open it", () => {
    dwell.enter("a");
    vi.advanceTimersByTime(400);
    dwell.leave("a");
    dwell.enter("b");
    vi.advanceTimersByTime(400);
    expect(dwell.opened).toBeNull();
    vi.advanceTimersByTime(DWELL_MS);
    expect(dwell.opened).toBe("b");
  });

  it("closes 150 ms after the pointer leaves, unless it comes back to the same entry", () => {
    dwell.enter("a");
    vi.advanceTimersByTime(DWELL_MS);
    dwell.leave("a");
    vi.advanceTimersByTime(CLOSE_MS - 10);
    dwell.enter("a");
    vi.advanceTimersByTime(CLOSE_MS * 4);
    expect(dwell.opened).toBe("a");
    dwell.leave("a");
    vi.advanceTimersByTime(CLOSE_MS);
    expect(dwell.opened).toBeNull();
  });

  it("resting on another entry closes the open one and opens that one in turn", () => {
    dwell.enter("a");
    vi.advanceTimersByTime(DWELL_MS);
    dwell.leave("a");
    dwell.enter("b");
    vi.advanceTimersByTime(CLOSE_MS);
    expect(dwell.opened).toBeNull();
    vi.advanceTimersByTime(DWELL_MS - CLOSE_MS);
    expect(dwell.opened).toBe("b");
  });

  it("the keys open after 1.4 s, only once they have moved, and each move starts over", () => {
    dwell.highlight("a", false);
    vi.advanceTimersByTime(KEY_DWELL_MS * 2);
    expect(dwell.opened).toBeNull();
    dwell.highlight("a", true);
    vi.advanceTimersByTime(KEY_DWELL_MS - 100);
    dwell.highlight("b", true);
    vi.advanceTimersByTime(KEY_DWELL_MS - 1);
    expect(dwell.opened).toBeNull();
    vi.advanceTimersByTime(1);
    expect(dwell.opened).toBe("b");
    dwell.highlight("c", true);
    expect(dwell.opened).toBeNull();
    expect(changes).toBeGreaterThan(0);
  });

  it("the pointer's entry wins over the keys'", () => {
    dwell.highlight("a", true);
    vi.advanceTimersByTime(KEY_DWELL_MS);
    dwell.enter("b");
    vi.advanceTimersByTime(DWELL_MS);
    expect(dwell.opened).toBe("b");
  });
});
