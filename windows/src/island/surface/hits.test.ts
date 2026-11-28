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

  it("keeps hit areas against the bottom and side anchors during growth", () => {
    const capsule = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "capsule", layout: "spacious" });
    // As at the top, the pointer is answered up to the screen edge the capsule floats from.
    const bottom = surfaceArea(capsule, open, closed, closed, 100, 50, "bottom");
    expect(bottom.reach.y + bottom.reach.height).toBe(50 + open.height);
    expect(bottom.reach.y + bottom.reach.height - (bottom.drawn.y + bottom.drawn.height)).toBeCloseTo(
      capsule.floatingGap!,
    );
    const tall = { width: 32, height: 200 };
    const right = surfaceArea(capsule, open, tall, tall, 100, 50, "right");
    expect(right.reach.x + right.reach.width).toBe(100 + open.width);
    expect(Math.abs(right.drawn.y + right.drawn.height / 2 - right.reach.y - right.reach.height / 2)).toBeLessThan(2);
    const left = surfaceArea(capsule, open, tall, tall, 100, 50, "left");
    expect(left.reach.x).toBe(100);
    expect(left.drawn.x).toBeCloseTo(100 + capsule.floatingGap!);
  });

  it("turns a notch's hit area onto a side edge, flat against it", () => {
    const panel = { width: 400, height: 300 };
    const area = surfaceArea(notch, panel, panel, panel, 0, 100, "left");
    // The ears take the panel's ends along the edge; the body runs the full depth.
    expect(area.drawn).toMatchObject({ x: 0, y: 114, width: 400, height: 272 });
    const right = surfaceArea(notch, panel, panel, panel, 1520, 100, "right");
    expect(right.drawn.x + right.drawn.width).toBe(1920);
  });

  it("answers to what is drawn while it shrinks, and to both when it has arrived", () => {
    const shrinking = surfaceArea(notch, closed, open, open, 100);
    expect(shrinking.reach).toEqual(shrinking.drawn);
    const arrived = surfaceArea(notch, open, open, open, 100);
    expect(arrived.reach).toEqual(arrived.drawn);
  });

  it("is cut to the notch as painted, ears and all, and answers to the same", () => {
    const area = surfaceArea(notch, open, open, open, 100);
    expect(area.drawn.ears).toEqual({ width: 14, rise: 14 });
    expect(area.reach.ears).toEqual(area.drawn.ears);
    // The ears flare out of the body, which is where the island's rect is.
    expect(area.drawn.x).toBe(100 + 14);
    expect(area.drawn.y).toBe(0);
  });

  it("is cut to the capsule without the strip above it, though the pointer is answered up to the screen edge", () => {
    const capsule = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "capsule", layout: "spacious" });
    const size = { width: 200, height: 32 };
    const area = surfaceArea(capsule, size, size, size, 100);
    expect(area.reach.y).toBe(0);
    expect(area.drawn.y).toBeGreaterThan(0);
    expect(area.drawn.y + area.drawn.height).toBeCloseTo(area.reach.y + area.reach.height, 5);
    expect(area.drawn.topRadius).toBeGreaterThan(0);
    expect(area.drawn.ears).toBeUndefined();
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

  it("sends the painted capsule beside its square-corner pointer reach", async () => {
    const capsule = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "capsule", layout: "spacious" });
    const area = surfaceArea(capsule, closed, closed, closed, 100);
    setSurface(area);
    await settle();
    expect(sent[0].rects[0].topRadius).toBe(0);
    expect(sent[0].drawn?.[0].topRadius).toBeGreaterThan(0);
    expect(sent[0].drawn?.[0].y).toBeGreaterThan(sent[0].rects[0].y);
  });

  it("keeps the previous visible strip interactive throughout a side transition", async () => {
    const capsule = makeGeometry({ screen, devicePixelRatio: 1.25, silhouette: "capsule", layout: "spacious" });
    const narrow = { width: 30, height: 200 };
    const wide = { width: 210, height: 320 };
    const old = surfaceArea(capsule, narrow, narrow, narrow, 100, 50, "left");
    setSurface(old);
    await settle();
    const growing = surfaceArea(capsule, wide, narrow, narrow, 100, 10, "left");
    setSurface(growing, false);
    await settle();
    await settle();
    expect(sent.at(-1)?.rects).toContainEqual(old.drawn);
    setSurface(surfaceArea(capsule, wide, wide, wide, 100, 10, "left"), true);
    await settle();
    await settle();
    expect(sent.at(-1)?.rects).not.toContainEqual(old.drawn);
  });

  it("does not let an older hit area overtake a newer frame", async () => {
    let finishFirst!: () => void;
    const first = new Promise<void>((resolve) => {
      finishFirst = resolve;
    });
    vi.mocked(bridge.setHitRects).mockImplementationOnce(async (rects, drawn) => {
      sent.push({ rects, drawn });
      await first;
    });
    const narrow = surfaceArea(notch, closed, closed, closed, 100);
    const wide = surfaceArea(notch, open, open, open, 100);
    setSurface(narrow);
    await settle();
    setSurface(wide);
    await settle();
    expect(sent).toHaveLength(1);
    finishFirst();
    await settle();
    await settle();
    await settle();
    expect(sent.at(-1)?.rects[0]).toEqual(wide.reach);
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
    await settle();
    await settle();
    expect(sent).toEqual([{ rects: [], drawn: [] }]);
  });
});
