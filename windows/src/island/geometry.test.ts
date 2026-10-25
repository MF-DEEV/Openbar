import { describe, expect, it } from "vitest";
import {
  capsulePath,
  contentInsets,
  notchMetrics,
  expandedHeight,
  expandedWidth,
  hitShape,
  hoverPulse,
  makeGeometry,
  notchPath,
  peekSize,
  restingSize,
} from "./geometry";

const screen = { width: 1920, height: 1080 };
const notch = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "notch", layout: "spacious" });
const capsule2x = makeGeometry({ screen, devicePixelRatio: 2, silhouette: "capsule", layout: "spacious" });
const capsule1x = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "capsule", layout: "spacious" });

describe("closed sizes (spec §1.4.1)", () => {
  it("capsule rests at 106 × 23.5 on 200% and 104 × 23 on 100%", () => {
    expect(restingSize(capsule2x)).toEqual({ width: 106, height: 23.5 });
    expect(restingSize(capsule1x)).toEqual({ width: 104, height: 23 });
  });

  it("simulated notch rests at 135 × 24, 223 with idle wings", () => {
    expect(restingSize(notch)).toEqual({ width: 135, height: 24 });
    expect(restingSize(notch, true)).toEqual({ width: 223, height: 24 });
  });

  it("hover pulses +20 wide, +5 tall only when hanging", () => {
    expect(hoverPulse(notch, restingSize(notch))).toEqual({ width: 155, height: 29 });
    expect(hoverPulse(capsule2x, restingSize(capsule2x))).toEqual({ width: 126, height: 23.5 });
  });

  it("peeks at 340 × 86 hanging and 340 × 68 as a capsule", () => {
    expect(peekSize(notch)).toEqual({ width: 340, height: 86 });
    expect(peekSize(capsule2x)).toEqual({ width: 340, height: 68 });
  });
});

describe("open sizes (spec §1.4.2)", () => {
  it("lists are 560 × 328 hanging and 560 × 340 as a capsule", () => {
    expect(expandedWidth(notch)).toBe(560);
    expect(expandedHeight(notch, { hasRow: true })).toBe(328);
    expect(expandedHeight(capsule2x, { hasRow: true })).toBe(340);
  });

  it("music, timer and controls narrow to 400", () => {
    expect(expandedWidth(notch, { natural: 344 })).toBe(400);
  });

  it("compact preset is 480 × 244 hanging", () => {
    const compact = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "notch", layout: "compact" });
    expect(expandedWidth(compact)).toBe(480);
    expect(expandedHeight(compact, { hasRow: true })).toBe(244);
  });

  it("the enlarged sessions page is 1180 × 864 on 1920 × 1080", () => {
    expect(expandedWidth(notch, { enlarged: true })).toBeCloseTo(1180, 0);
    expect(expandedHeight(notch, { hasRow: true, enlarged: true })).toBe(864);
  });
});

describe("outlines", () => {
  it("keep the same number of elements at every size so frames blend", () => {
    const count = (p: string) => p.split(/[MLCZ]/).length;
    expect(count(notchPath(135, 24))).toBe(count(notchPath(560, 328)));
    expect(count(capsulePath(106, 23.5, 2))).toBe(count(capsulePath(560, 340, 2)));
    expect(count(capsulePath(0, 0, 2))).toBe(count(capsulePath(560, 340, 2)));
  });

  it("hang from the top edge across the ears", () => {
    expect(notchPath(135, 24).startsWith("M0 0 L135 0")).toBe(true);
  });

  it("hit the body between the shoulders, from the very top", () => {
    const hit = hitShape(notch, { width: 560, height: 328 });
    expect(hit.rect).toEqual({ x: 14, y: 0, width: 532, height: 328 });
    expect(hit.bottomRadius).toBe(28);
    const cap = hitShape(capsule2x, { width: 106, height: 23.5 });
    expect(cap.rect.y).toBe(0);
    expect(cap.rect.height).toBeCloseTo(21.5, 5);
  });
});

describe("custom island shape", () => {
  it("resizes the closed notch and preserves room for live activity wings", () => {
    const custom = makeGeometry({ ...notch.input, notchWidth: 260, notchHeight: 40 });
    expect(restingSize(custom)).toEqual({ width: 260, height: 40 });
    expect(restingSize(custom, true)).toEqual({ width: 348, height: 40 });
  });

  it("keeps the drawn contour and pointer area in sync", () => {
    const custom = makeGeometry({ ...notch.input, roundness: 0, shoulderWidth: 200, shoulderHeight: 150 });
    const size = { width: 560, height: 328 };
    const hit = hitShape(custom, size);
    expect(hit.rect).toEqual({ x: 28, y: 0, width: 504, height: 328 });
    expect(hit.bottomRadius).toBe(0);
    expect(notchPath(size.width, size.height, custom.input)).toContain("L532 328");
    expect(notchPath(size.width, size.height, custom.input)).toContain("L28 21");
  });

  it("bounds extreme curves even on a zero-sized animation frame", () => {
    const contour = { roundness: 200, shoulderWidth: 300, shoulderHeight: 300 };
    for (const size of [
      { width: 0, height: 0 },
      { width: 10, height: 4 },
    ]) {
      expect(notchPath(size.width, size.height, contour)).not.toMatch(/NaN|Infinity/);
      const hit = hitShape(makeGeometry({ ...notch.input, ...contour }), size);
      expect(hit.rect.width).toBeGreaterThanOrEqual(0);
      expect(hit.bottomRadius).toBeGreaterThanOrEqual(0);
    }
  });

  it("applies capsule rounding to both the path and pointer area", () => {
    const custom = makeGeometry({ ...capsule1x.input, roundness: 0 });
    expect(hitShape(custom, restingSize(custom)).bottomRadius).toBe(0);
    expect(capsulePath(106, 24, 2, 0)).toContain("M4.5600000000000005 2");
  });
});

describe("content fits the configured silhouette", () => {
  it("preserves the default content spacing", () => {
    expect(contentInsets(notch, { width: 560, height: 328 })).toEqual({ horizontal: 28, bottom: 16 });
  });

  it("keeps content inside stretched sides and rounded bottom corners at every open width", () => {
    for (const width of [240, 360, 400, 600, 1180]) {
      for (const height of [140, 260, 640]) {
        const geometry = makeGeometry({ ...notch.input, shoulderWidth: 300, shoulderHeight: 300, roundness: 200 });
        const { horizontal, bottom } = contentInsets(geometry, { width, height });
        const { s, b } = notchMetrics(width, height, geometry.input);
        expect(horizontal).toBeGreaterThanOrEqual(s + 12);
        expect(width - 2 * horizontal).toBeGreaterThan(0);
        const dx = Math.max(0, s + b - horizontal);
        const dy = Math.max(0, b - bottom);
        expect(dx * dx + dy * dy).toBeLessThanOrEqual(b * b);
      }
    }
  });

  it("includes the capsule gap when reserving rounded bottom space", () => {
    const geometry = makeGeometry({ ...capsule1x.input, roundness: 200 });
    const insets = contentInsets(geometry, { width: 360, height: 260 });
    expect(insets.horizontal).toBe(42);
    expect(insets.bottom).toBe(30);
  });
});
