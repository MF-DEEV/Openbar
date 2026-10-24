import { describe, expect, it } from "vitest";
import {
  arrivalTime,
  limitedSpring,
  MotionTimeline,
  OVERSHOOT_LIMIT,
  settlingTime,
  sideSpring,
  sizeAt,
  springOvershoot,
  springProgress,
  Springs,
} from "./motion";

const rest = { width: 106, height: 23.5 };
const list = { width: 560, height: 340 };

describe("springs", () => {
  it("start at rest and end at the target", () => {
    for (const s of Object.values(Springs)) {
      expect(springProgress(s, 0)).toBe(0);
      expect(springProgress(s, 2)).toBeCloseTo(1, 4);
    }
  });

  it("growing width swings by about 2.84%", () => {
    expect(springOvershoot(Springs.growingWidth)).toBeCloseTo(0.0284, 3);
    expect(springOvershoot(Springs.shrinkingWidth)).toBe(0);
  });

  it("limit a long swing to 12 points", () => {
    // spec §3.1.2: 106 → 560 gives bounce 0.244
    const limited = limitedSpring(Springs.growingWidth, 454, OVERSHOOT_LIMIT);
    expect(limited.bounce).toBeCloseTo(0.244, 3);
    expect(454 * springOvershoot(limited)).toBeCloseTo(12, 5);
  });

  it("pick by direction", () => {
    expect(sideSpring(10, 20, true)).toEqual(Springs.growingWidth);
    expect(sideSpring(20, 10, false)).toEqual(Springs.shrinkingHeight);
    expect(sideSpring(20, 10, true, true)).toEqual(Springs.steadyWidth);
  });
});

describe("resize timings match the Mac (spec §3.1.8)", () => {
  it("opens the capsule to a list in ~562 ms, arriving at ~250 ms", () => {
    expect(settlingTime(rest, list) * 1000).toBeGreaterThan(550);
    expect(settlingTime(rest, list) * 1000).toBeLessThan(575);
    expect(arrivalTime(rest, list) * 1000).toBeGreaterThan(240);
    expect(arrivalTime(rest, list) * 1000).toBeLessThan(260);
  });

  it("closes a list to rest in ~437 ms, never passing the target", () => {
    expect(settlingTime(list, rest) * 1000).toBeGreaterThan(425);
    expect(settlingTime(list, rest) * 1000).toBeLessThan(450);
    for (let t = 0; t < 1; t += 0.005) {
      const s = sizeAt(t, list, rest);
      expect(s.width).toBeGreaterThanOrEqual(rest.width - 1e-9);
      expect(s.height).toBeGreaterThanOrEqual(rest.height - 1e-9);
    }
  });

  it("pulses the notch on hover in ~375 ms and back in ~267 ms", () => {
    const a = { width: 135, height: 24 };
    const b = { width: 155, height: 29 };
    expect(settlingTime(a, b) * 1000).toBeCloseTo(375, -1);
    expect(settlingTime(b, a) * 1000).toBeCloseTo(267, -1);
  });

  it("never swings more than 12 points past the target", () => {
    const big = { width: 1400, height: 900 };
    for (let t = 0; t < 2; t += 0.004) {
      const s = sizeAt(t, { width: 200, height: 32 }, big);
      expect(s.width).toBeLessThanOrEqual(big.width + OVERSHOOT_LIMIT + 1e-9);
      expect(s.height).toBeLessThanOrEqual(big.height + OVERSHOOT_LIMIT + 1e-9);
    }
  });
});

describe("timeline", () => {
  it("follows a single spring until retargeted", () => {
    const tl = MotionTimeline.create(rest, list, 10);
    for (const t of [0, 0.05, 0.2, 0.5]) {
      const a = tl.sizeAt(10 + t);
      const b = sizeAt(t, rest, list);
      expect(a.width).toBeCloseTo(b.width, 9);
      expect(a.height).toBeCloseTo(b.height, 9);
    }
  });

  it("carries on when the target moves the same way, without stalling", () => {
    const tl = MotionTimeline.create(rest, { width: 400, height: 200 }, 0);
    const next = tl.continued(list, 0.1);
    expect(next).not.toBeNull();
    const at = tl.sizeAt(0.1);
    const after = next!.sizeAt(0.1 + 1 / 120);
    expect(after.width).toBeGreaterThan(at.width);
    expect(next!.target).toEqual(list);
    const end = next!.sizeAt(3);
    expect(end.width).toBeCloseTo(list.width, 3);
  });

  it("starts again when a side turns back", () => {
    const tl = MotionTimeline.create(rest, list, 0);
    expect(tl.continued(rest, 0.1)).toBeNull();
    expect(tl.continued({ width: 600, height: 100 }, 0.1)).toBeNull();
  });

  it("is never carried on when steady or moving its centre", () => {
    expect(MotionTimeline.create(rest, list, 0, true).continued(list, 0.1)).toBeNull();
    expect(MotionTimeline.create(rest, list, 0, false, 40).continued(list, 0.1)).toBeNull();
  });

  it("settles after its settling duration", () => {
    const tl = MotionTimeline.create(rest, list, 5);
    expect(tl.settledAt(5.1)).toBe(false);
    expect(tl.settledAt(5 + tl.settlingDuration())).toBe(true);
  });
});
