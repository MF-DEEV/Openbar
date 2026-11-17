// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Scrolling opens and closes the island; a sideways swipe over the music
// changes tracks (NotchGestureSupport.swift). One decision per physical
// scroll sequence. Windows sends no scroll phases, so the next event itself
// expires an old sequence, and a swipe's inertia is the same sequence.

export type GestureAction = "open" | "close" | "nextTrack" | "previousTrack";

/** Longest pause inside one physical scroll, in seconds. */
const SEQUENCE_GAP = 0.35;
const VERTICAL_THRESHOLD = 16;
const HORIZONTAL_THRESHOLD = 40;
/** Travel that establishes intent, so touching the pad can't pick the axis. */
const INTENT = 4;
/** One wheel notch counts as this much travel. */
const NOTCH = 24;

export interface WheelInput {
  deltaX: number;
  deltaY: number;
  deltaMode: number;
  /** Legacy fields: whole multiples of 120 from a wheel, fractions of it from a touchpad. */
  wheelDeltaX?: number;
  wheelDeltaY?: number;
}

/** A precision touchpad reports fine-grained travel; a wheel, whole notches. */
export function isPrecise(event: WheelInput): boolean {
  if (event.deltaMode !== 0) return false;
  return (event.wheelDeltaX ?? 0) % 120 !== 0 || (event.wheelDeltaY ?? 0) % 120 !== 0;
}

/**
 * Travel in the fingers' direction: positive is down or right. Windows's default
 * touchpad direction moves the content with the fingers, so the delta is
 * opposite to them. A wheel turned towards the person pulls the island down.
 */
export function movement(event: WheelInput): { x: number; y: number; precise: boolean } {
  const precise = isPrecise(event);
  const finite = (value: number) => (Number.isFinite(value) ? value : 0);
  if (precise) return { x: -finite(event.deltaX), y: -finite(event.deltaY), precise };
  return { x: Math.sign(finite(event.deltaX)) * -NOTCH, y: Math.sign(finite(event.deltaY)) * NOTCH, precise };
}

export function allowsVertical(place: {
  expanded: boolean;
  inHeader: boolean;
  musicSurface: boolean;
  control: boolean;
  scroll: boolean;
}): boolean {
  return !place.control && (!place.expanded || place.inHeader || (place.musicSurface && !place.scroll));
}

export function allowsHorizontal(place: {
  inHeader: boolean;
  musicSurface: boolean;
  control: boolean;
  scroll: boolean;
}): boolean {
  return !place.control && !place.scroll && !place.inHeader && place.musicSurface;
}

export class GestureTracker {
  private axis: "horizontal" | "vertical" | null = null;
  private pendingX = 0;
  private pendingY = 0;
  private distance = 0;
  private fired = false;
  private last: number | null = null;

  reset(): void {
    this.axis = null;
    this.pendingX = 0;
    this.pendingY = 0;
    this.distance = 0;
    this.fired = false;
    this.last = null;
  }

  /** `timestamp` is in seconds. */
  handle(input: {
    x: number;
    y: number;
    timestamp: number;
    precise: boolean;
    allowVertical: boolean;
    allowHorizontal: boolean;
    expanded: boolean;
  }): GestureAction | null {
    const { x, y, timestamp, precise, allowVertical, allowHorizontal, expanded } = input;
    if (!Number.isFinite(timestamp) || !Number.isFinite(x) || !Number.isFinite(y)) {
      this.reset();
      return null;
    }
    if (this.last !== null && (timestamp < this.last || timestamp - this.last > SEQUENCE_GAP)) this.reset();
    this.last = timestamp;
    if (this.fired) return null;
    if (this.axis === null) {
      this.pendingX += x;
      this.pendingY += y;
      const [absX, absY] = [Math.abs(this.pendingX), Math.abs(this.pendingY)];
      if (precise && allowHorizontal && absX >= INTENT && absX >= absY * 1.5) {
        this.axis = "horizontal";
        this.distance = this.pendingX;
      } else if (allowVertical && absY >= INTENT && absY >= absX * 1.5) {
        this.axis = "vertical";
        this.distance = this.pendingY;
      } else {
        return null;
      }
    } else {
      this.distance += this.axis === "horizontal" ? x : y;
    }
    if (this.axis === "horizontal" && allowHorizontal) {
      if (Math.abs(this.distance) < HORIZONTAL_THRESHOLD) return null;
      this.fired = true;
      return this.distance < 0 ? "nextTrack" : "previousTrack";
    }
    if (this.axis === "vertical" && allowVertical) {
      if (Math.abs(this.distance) < VERTICAL_THRESHOLD) return null;
      this.fired = true;
      if (this.distance > 0 && !expanded) return "open";
      if (this.distance < 0 && expanded) return "close";
    }
    return null;
  }
}
