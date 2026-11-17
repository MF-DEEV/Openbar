// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { allowsHorizontal, allowsVertical, GestureTracker, isPrecise, movement } from "./gestures";

function feeder() {
  const tracker = new GestureTracker();
  let time = 1;
  const feed = (
    x: number,
    y: number,
    o: { vertical?: boolean; horizontal?: boolean; expanded?: boolean; precise?: boolean; gap?: number } = {},
  ) => {
    time += o.gap ?? 0.01;
    return tracker.handle({
      x,
      y,
      timestamp: time,
      precise: o.precise ?? true,
      allowVertical: o.vertical ?? true,
      allowHorizontal: o.horizontal ?? true,
      expanded: o.expanded ?? false,
    });
  };
  return feed;
}
const NEW = { gap: 1 };

describe("gesture tracker (NotchGestureTests)", () => {
  it("accumulates small vertical movement into one opening", () => {
    const feed = feeder();
    expect([feed(0, 5), feed(0, 10), feed(0, 10)]).toEqual([null, null, "open"]);
  });

  it("the rest of an opening swipe, its inertia included, cannot close the island", () => {
    const feed = feeder();
    feed(0, 30);
    expect(feed(0, -100, { expanded: true })).toBeNull();
    expect(feed(0, -80, { expanded: true })).toBeNull();
    expect(feed(0, -30, { expanded: true, ...NEW })).toBe("close");
  });

  it("a left swipe skips forward once, a right one back", () => {
    const feed = feeder();
    expect([feed(-10, 0), feed(-15, 0), feed(-20, 0)]).toEqual([null, null, "nextTrack"]);
    for (let i = 0; i < 100; i++) expect(feed(-50, 0)).toBeNull();
    expect(feed(45, 0, NEW)).toBe("previousTrack");
  });

  it("does not guess diagonals, and touchdown noise does not lock an axis", () => {
    const feed = feeder();
    expect(feed(40, 40)).toBeNull();
    expect(feed(1, 0, { expanded: true, ...NEW })).toBeNull();
    expect(feed(0, -24, { expanded: true })).toBe("close");
    expect(feed(0, 1, NEW)).toBeNull();
    expect(feed(-40, 0)).toBe("nextTrack");
  });

  it("slow travel adds up", () => {
    const feed = feeder();
    for (let i = 0; i < 8; i++) expect(feed(0, 0.5)).toBeNull();
    expect(feed(0, 21)).toBe("open");
  });

  it("a wheel never skips a track", () => {
    const feed = feeder();
    expect(feed(-100, 0, { precise: false })).toBeNull();
  });

  it("a blocked direction does nothing", () => {
    const feed = feeder();
    expect(feed(0, 50, { vertical: false })).toBeNull();
    expect(feed(-50, 0, { horizontal: false, ...NEW })).toBeNull();
  });

  it("opening when open and closing when closed do nothing", () => {
    const feed = feeder();
    expect(feed(0, 50, { expanded: true })).toBeNull();
    expect(feed(0, -50, NEW)).toBeNull();
  });

  it("a clock going back or a bad value starts over", () => {
    const tracker = new GestureTracker();
    const input = { x: 0, precise: true, allowVertical: true, allowHorizontal: true, expanded: false };
    expect(tracker.handle({ ...input, y: 30, timestamp: 5 })).toBe("open");
    expect(tracker.handle({ ...input, y: 30, timestamp: 4 })).toBe("open");
    expect(tracker.handle({ ...input, y: NaN, timestamp: 4.01 })).toBeNull();
    expect(tracker.handle({ ...input, y: 30, timestamp: 4.02 })).toBe("open");
  });
});

describe("input reading", () => {
  it("tells a wheel from a touchpad", () => {
    expect(isPrecise({ deltaX: 0, deltaY: 100, deltaMode: 0, wheelDeltaX: 0, wheelDeltaY: -120 })).toBe(false);
    expect(isPrecise({ deltaX: 0, deltaY: 7, deltaMode: 0, wheelDeltaX: 0, wheelDeltaY: -8 })).toBe(true);
    expect(isPrecise({ deltaX: 3, deltaY: 0, deltaMode: 0, wheelDeltaX: -4, wheelDeltaY: 0 })).toBe(true);
    expect(isPrecise({ deltaX: 0, deltaY: 3, deltaMode: 1, wheelDeltaX: 0, wheelDeltaY: -8 })).toBe(false);
  });

  it("a wheel turned down opens, whatever its size", () => {
    expect(movement({ deltaX: 0, deltaY: 300, deltaMode: 0, wheelDeltaY: -360 }).y).toBe(24);
    expect(movement({ deltaX: 0, deltaY: -100, deltaMode: 0, wheelDeltaY: 120 }).y).toBe(-24);
  });

  it("touchpad travel follows the fingers at Windows's default direction", () => {
    expect(movement({ deltaX: 0, deltaY: -7, deltaMode: 0, wheelDeltaY: 8 }).y).toBe(7);
    expect(movement({ deltaX: 7, deltaY: 0, deltaMode: 0, wheelDeltaX: -8 }).x).toBe(-7);
  });
});

describe("where gestures apply", () => {
  const place = { expanded: false, inHeader: false, musicSurface: false, control: false, scroll: false };
  it("the closed island and an open header take vertical gestures", () => {
    expect(allowsVertical(place)).toBe(true);
    expect(allowsVertical({ ...place, expanded: true, inHeader: true })).toBe(true);
  });
  it("the music surface closes, other pages, lists and controls keep their input", () => {
    expect(allowsVertical({ ...place, expanded: true, musicSurface: true })).toBe(true);
    expect(allowsVertical({ ...place, expanded: true })).toBe(false);
    expect(allowsVertical({ ...place, expanded: true, musicSurface: true, scroll: true })).toBe(false);
    expect(allowsVertical({ ...place, control: true })).toBe(false);
  });
  it("skips only over the music, away from the header, lists and controls", () => {
    expect(allowsHorizontal({ ...place, musicSurface: true })).toBe(true);
    expect(allowsHorizontal(place)).toBe(false);
    expect(allowsHorizontal({ ...place, musicSurface: true, inHeader: true })).toBe(false);
    expect(allowsHorizontal({ ...place, musicSurface: true, scroll: true })).toBe(false);
    expect(allowsHorizontal({ ...place, musicSurface: true, control: true })).toBe(false);
  });
});
