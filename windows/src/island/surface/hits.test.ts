import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { bridge, type HitRect } from "../../shared/bridge";
import { makeGeometry } from "../geometry";
import { circleDrawn, clearAreas, LAG_MS, setCircles, setCircleScale, setSurface, surfaceArea, Trailing } from "./hits";

const screen = { width: 1920, height: 1080 };
const notch = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "notch", layout: "spacious" });
const open = { width: 560, height: 328 };
const closed = { width: 200, height: 32 };

const disc = (x: number, y: number): HitRect => ({
  x,
  y,
  width: 44,
  height: 44,
  topRadius: 22,
  bottomRadius: 22,
});

describe("a growing island", () => {
  it("answers to where it is going but is drawn where it is", () => {
    const area = surfaceArea(notch, open, closed, closed, 100);
    expect(area.reach.width).toBeGreaterThan(area.drawn.width);
    expect(area.reach.height).toBe(open.height);
    expect(area.drawn.height).toBeLessThan(open.height);
    // Both are centred on the same line.
    expect(area.reach.x + area.reach.width / 2).toBeCloseTo(area.drawn.x + area.drawn.width / 2);
  });

  it("answers to what is drawn while it shrinks, and to both when it has arrived", () => {
    const shrinking = surfaceArea(notch, closed, open, open, 100);
    expect(shrinking.reach).toEqual(shrinking.drawn);
    const arrived = surfaceArea(notch, open, open, open, 100);
    expect(arrived.reach).toEqual(arrived.drawn);
  });

  it("is cut to the held size, which answers to nothing larger than what is drawn", () => {
    const middle = { width: 380, height: 180 };
    const area = surfaceArea(notch, open, middle, closed, 100);
    expect(area.drawn.width).toBeLessThan(surfaceArea(notch, open, middle, middle, 100).drawn.width);
    expect(area.reach).toEqual(surfaceArea(notch, open, middle, middle, 100).reach);
  });
});

describe("the cut trailing the drawing", () => {
  const at = (n: number) => n * 16;
  const grow = (n: number) => ({ width: 200 + n * 30, height: 32 + n * 20 });

  it("holds a growing island back to what the last frames drew, so the cut is never ahead of the screen", () => {
    const trail = new Trailing();
    const held = Array.from({ length: 12 }, (_, n) => trail.push(grow(n), at(n)));
    held.forEach((size, n) => {
      expect(size.width).toBeLessThanOrEqual(grow(n).width);
      expect(size.height).toBeLessThanOrEqual(grow(n).height);
    });
    // Once the lag has passed, the cut is the oldest size still inside it.
    const frames = Math.floor(LAG_MS / 16);
    expect(held[11]).toEqual(grow(11 - frames));
  });

  it("cuts a shrinking island at once", () => {
    const trail = new Trailing();
    for (let n = 0; n < 8; n++) trail.push(grow(7 - n), at(n));
    expect(trail.push(grow(-1), at(8))).toEqual(grow(-1));
  });

  it("takes width and height on their own", () => {
    const trail = new Trailing();
    trail.push({ width: 300, height: 100 }, 0);
    expect(trail.push({ width: 200, height: 160 }, 16)).toEqual({ width: 200, height: 100 });
  });

  it("forgets what is older than the lag and what came before it arrived", () => {
    const trail = new Trailing();
    trail.push({ width: 100, height: 40 }, 0);
    expect(trail.push({ width: 400, height: 300 }, LAG_MS + 1)).toEqual({ width: 400, height: 300 });
    trail.push({ width: 100, height: 40 }, LAG_MS + 2);
    trail.reset();
    expect(trail.push({ width: 400, height: 300 }, LAG_MS + 3)).toEqual({ width: 400, height: 300 });
  });
});

describe("a round button", () => {
  it("is drawn smaller around its own centre while it grows in", () => {
    const full = disc(100, 200);
    expect(circleDrawn(full, 1)).toEqual(full);
    const part = circleDrawn(full, 0.5);
    expect(part).toEqual({ x: 111, y: 211, width: 22, height: 22, topRadius: 11, bottomRadius: 11 });
  });
});

describe("what goes out to Rust", () => {
  let sent: { rects: HitRect[]; drawn?: HitRect[] }[];
  const settle = () => Promise.resolve();

  beforeEach(() => {
    sent = [];
    vi.spyOn(bridge, "setHitRects").mockImplementation(async (rects, drawn) => {
      sent.push({ rects, drawn });
    });
    clearAreas();
    sent = [];
  });
  afterEach(() => vi.restoreAllMocks());

  it("sends what changed in one frame as one message", async () => {
    const area = surfaceArea(notch, open, closed, closed, 100);
    setSurface(area);
    setCircles([{ id: "a", reach: disc(10, 10) }]);
    setCircleScale("a", 0.5);
    await settle();
    expect(sent).toHaveLength(1);
    expect(sent[0].rects).toEqual([area.reach, disc(10, 10)]);
    expect(sent[0].drawn).toEqual([area.drawn, circleDrawn(disc(10, 10), 0.5)]);
  });

  it("leaves a button out of the drawing until it is on screen, and again when it is gone", async () => {
    const area = surfaceArea(notch, open, open, open, 100);
    setSurface(area);
    setCircles([
      { id: "a", reach: disc(10, 10) },
      { id: "b", reach: disc(70, 10) },
    ]);
    await settle();
    expect(sent[0].rects).toHaveLength(3);
    expect(sent[0].drawn).toEqual([area.drawn]);

    setCircleScale("b", 1);
    await settle();
    expect(sent[1].drawn).toEqual([area.drawn, disc(70, 10)]);

    setCircleScale("b", 0);
    await settle();
    expect(sent[2].drawn).toEqual([area.drawn]);
  });

  it("does not repeat a message that says nothing new", async () => {
    setSurface(surfaceArea(notch, open, closed, closed, 100));
    await settle();
    setSurface(surfaceArea(notch, open, closed, closed, 100));
    setCircleScale("a", 0);
    await settle();
    expect(sent).toHaveLength(1);
  });

  it("takes everything back when the island is off", async () => {
    setSurface(surfaceArea(notch, open, closed, closed, 100));
    setCircles([{ id: "a", reach: disc(10, 10) }]);
    await settle();
    sent = [];
    clearAreas();
    expect(sent).toEqual([{ rects: [], drawn: [] }]);
  });
});
