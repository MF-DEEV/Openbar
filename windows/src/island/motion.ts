// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's resize motion, ported from NotchMotion / NotchMotionTimeline
// (Sources/Vorssaint/Services/Notch/NotchSupport.swift). Width and height
// each ride a spring of their own, chosen by direction; a growing side may
// swing past its target, a shrinking one never does.

export interface Size {
  width: number;
  height: number;
}

/** Perceptual duration and bounce, as SwiftUI and Core Animation define them. */
export interface Spring {
  duration: number;
  bounce: number;
}

/** Progress from rest at 0 toward 1, `time` in seconds. */
export function springProgress(spring: Spring, time: number): number {
  if (time <= 0) return 0;
  const natural = (2 * Math.PI) / spring.duration;
  const damping = 1 - spring.bounce;
  if (damping >= 1) return 1 - Math.exp(-natural * time) * (1 + natural * time);
  const damped = natural * Math.sqrt(1 - damping * damping);
  return (
    1 -
    Math.exp(-damping * natural * time) *
      (Math.cos(damped * time) + ((damping * natural) / damped) * Math.sin(damped * time))
  );
}

/** How far past the target the spring swings, as a share of its travel. */
export function springOvershoot(spring: Spring): number {
  if (spring.bounce <= 0) return 0;
  const damping = 1 - spring.bounce;
  return Math.exp((-Math.PI * damping) / Math.sqrt(1 - damping * damping));
}

/** The spring with only as much bounce as keeps the swing within `limit` points. */
export function limitedSpring(spring: Spring, travel: number, limit: number): Spring {
  if (spring.bounce <= 0 || travel <= 0 || travel * springOvershoot(spring) <= limit) return spring;
  const share = Math.log(Math.max(limit, 0.01) / travel);
  return { duration: spring.duration, bounce: 1 + share / Math.sqrt(Math.PI * Math.PI + share * share) };
}

export const Springs = {
  growingWidth: { duration: 0.44, bounce: 0.25 },
  growingHeight: { duration: 0.38, bounce: 0.22 },
  shrinkingWidth: { duration: 0.3, bounce: 0 },
  shrinkingHeight: { duration: 0.26, bounce: 0 },
  /** A strip that stays on screen and only fits a new reading eases without a swing. */
  steadyWidth: { duration: 0.5, bounce: 0 },
} as const satisfies Record<string, Spring>;

/** The farthest a side may pass its target. */
export const OVERSHOOT_LIMIT = 12;
/** Sides closer than this to their targets read as settled. */
export const SETTLED_DISTANCE = 0.5;
/** Departing content has faded out by 0.16 s; the view swaps it at 0.2 s. */
export const DEPARTURE_HIDDEN = 0.2;

export function sideSpring(from: number, to: number, width: boolean, steady = false): Spring {
  if (steady && width) return Springs.steadyWidth;
  const spring =
    to > from
      ? width
        ? Springs.growingWidth
        : Springs.growingHeight
      : width
        ? Springs.shrinkingWidth
        : Springs.shrinkingHeight;
  return limitedSpring(spring, Math.abs(to - from), OVERSHOOT_LIMIT);
}

/** The spring carrying the island's sides, for controls that ride along them. */
export function carryingSpring(from: Size, to: Size): Spring {
  return from.width !== to.width ? sideSpring(from.width, to.width, true) : sideSpring(from.height, to.height, false);
}

export function sizeAt(time: number, from: Size, to: Size, steady = false): Size {
  const side = (start: number, end: number, width: boolean) => {
    if (start === end) return end;
    const spring = sideSpring(start, end, width, steady);
    return Math.max(0, start + (end - start) * springProgress(spring, time));
  };
  return { width: side(from.width, to.width, true), height: side(from.height, to.height, false) };
}

/** When every moving side first comes within 1% of its travel: arrived, though it may still swing. */
export function arrivalTime(from: Size, to: Size): number {
  const sides: [number, number, boolean][] = (
    [
      [from.width, to.width, true],
      [from.height, to.height, false],
    ] as [number, number, boolean][]
  ).filter(([a, b]) => a !== b);
  if (sides.length === 0) return 0;
  const step = 1 / 240;
  let time = step;
  while (time < 2 && !sides.every(([a, b, w]) => springProgress(sideSpring(a, b, w), time) >= 0.99)) {
    time += step;
  }
  return time;
}

/** How far a moving island's centre still sits from its new one. */
export function offsetAt(time: number, from: Size, to: Size, start: number, steady = false): number {
  if (start === 0) return 0;
  const spring = from.width === to.width ? Springs.steadyWidth : sideSpring(from.width, to.width, true, steady);
  return start * (1 - springProgress(spring, time));
}

/** When size and centre stay within SETTLED_DISTANCE of their targets for good. */
export function settlingTime(from: Size, to: Size, steady = false, startOffset = 0): number {
  const step = 1 / 240;
  let settled = step;
  for (let time = step; time < 2; time += step) {
    const size = sizeAt(time, from, to, steady);
    const offset = offsetAt(time, from, to, startOffset, steady);
    if (
      Math.abs(size.width - to.width) > SETTLED_DISTANCE ||
      Math.abs(size.height - to.height) > SETTLED_DISTANCE ||
      Math.abs(offset) > SETTLED_DISTANCE
    ) {
      settled = time + step;
    }
  }
  return settled;
}

interface Leg {
  begin: number;
  from: number;
  to: number;
}

/**
 * A resize in flight, each side a sum of springs from rest: the move it began
 * with, and one more from each moment that side's target moved on the way it
 * was already going. A motion turning back on either side starts again from
 * where it is drawn, at rest (`continued` returns null).
 */
export class MotionTimeline {
  private constructor(
    private readonly widthLegs: Leg[],
    private readonly heightLegs: Leg[],
    /** When its latest move began, and the size there. */
    readonly begin: number,
    readonly start: Size,
    readonly steady: boolean,
    /** How far the centre still sat from its new one when the motion began. */
    readonly offset: number,
  ) {}

  static create(from: Size, to: Size, begin: number, steady = false, offset = 0): MotionTimeline {
    return new MotionTimeline(
      [{ begin, from: from.width, to: to.width }],
      [{ begin, from: from.height, to: to.height }],
      begin,
      from,
      steady,
      offset,
    );
  }

  get target(): Size {
    return {
      width: this.widthLegs.at(-1)?.to ?? this.start.width,
      height: this.heightLegs.at(-1)?.to ?? this.start.height,
    };
  }

  sizeAt(time: number): Size {
    return {
      width: this.value(this.widthLegs, time, true),
      height: this.value(this.heightLegs, time, false),
    };
  }

  offsetAt(time: number): number {
    return offsetAt(time - this.begin, this.start, this.target, this.offset, this.steady);
  }

  /** True once every side and the centre stay within SETTLED_DISTANCE for good. */
  settledAt(time: number): boolean {
    return time - this.begin >= this.settlingDuration() - 1e-9;
  }

  private settling?: number;

  /** Seconds from the latest move until every side stays settled. */
  settlingDuration(): number {
    if (this.settling !== undefined) return this.settling;
    const target = this.target;
    const step = 1 / 240;
    let duration = step;
    for (let t = step; t < 2; t += step) {
      const size = this.sizeAt(this.begin + t);
      if (
        Math.abs(size.width - target.width) > SETTLED_DISTANCE ||
        Math.abs(size.height - target.height) > SETTLED_DISTANCE ||
        Math.abs(this.offsetAt(this.begin + t)) > SETTLED_DISTANCE
      ) {
        duration = t + step;
      }
    }
    this.settling = duration;
    return duration;
  }

  /** This motion headed for `size` from `time` on, or null when a side turns back. */
  continued(size: Size, time: number): MotionTimeline | null {
    if (this.steady || this.offset !== 0) return null;
    const width = continuedLegs(this.widthLegs, size.width, time);
    const height = continuedLegs(this.heightLegs, size.height, time);
    if (!width || !height) return null;
    return new MotionTimeline(width, height, time, this.sizeAt(time), this.steady, this.offset);
  }

  private value(legs: Leg[], time: number, width: boolean): number {
    const first = legs[0];
    const last = legs.at(-1);
    if (!first || !last) return 0;
    let value = first.from;
    for (const leg of legs) {
      if (leg.from === leg.to) continue;
      const spring = sideSpring(leg.from, leg.to, width, this.steady);
      value += (leg.to - leg.from) * springProgress(spring, time - leg.begin);
    }
    // Swings that coincide still pass the target no further than one does.
    return Math.max(0, Math.min(value, Math.max(first.from, last.to) + OVERSHOOT_LIMIT));
  }
}

function continuedLegs(legs: Leg[], end: number, time: number): Leg[] | null {
  const last = legs.at(-1);
  if (!last || end === last.to) return legs;
  const moving = legs.find((leg) => leg.from !== leg.to);
  const heading = moving ? moving.to - moving.from : 0;
  if (heading !== 0 && end > last.to !== heading > 0) return null;
  return [...legs, { begin: time, from: last.to, to: end }];
}
