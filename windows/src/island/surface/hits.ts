// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the page tells Rust about each shape it draws, in two parts: where the
// pointer is answered (`reach`) and what is on screen right now (`drawn`).
// They differ while something grows. Hover must not let go of an island on its
// way to a bigger size, but a compositor that blurs behind the window (picom on
// X11) may only see what is drawn, or a frosted ghost of the final size shows
// before the island has grown into it (island.rs, `reshape`).

import { bridge, type HitRect } from "../../shared/bridge";
import { type Geometry, edgeHitShape } from "../geometry";
import type { Size } from "../motion";
import { surfaceOffset, type IslandEdge, type IslandPosition } from "../placement";

export interface Area {
  reach: HitRect;
  drawn: HitRect;
}

/**
 * The window is cut to what is drawn (X11), and the compositor blurs behind
 * everything inside the cut, translucent pixels included: what appears there
 * must not fade in. This page runs in WebKitGTK only on Linux.
 */
export const windowIsCut = typeof navigator !== "undefined" && /\bLinux\b/.test(navigator.userAgent);
// The cut is exactly what is painted, so it clips an outline drawn outside a button: island.css moves it in.
if (windowIsCut && typeof document !== "undefined") document.documentElement.classList.add("window-cut");

/**
 * The outline of an island of `size`, centred in a frame of the target's width that starts at `left`:
 * where the pointer is answered, or what is painted.
 */
type OffsetOf = (size: Size) => { x: number; y: number };

function outlineAt(
  geometry: Geometry,
  target: Size,
  left: number,
  size: Size,
  part: "reach" | "painted",
  top = geometry.floatingDrop,
  edge: IslandEdge = "top",
  position?: IslandPosition,
  offsetOf?: OffsetOf,
): HitRect {
  const hit = edgeHitShape(geometry, size, edge);
  const shape = part === "painted" ? hit.painted : hit;
  const offset = offsetOf ? offsetOf(size) : surfaceOffset(target, size, edge, position);
  return {
    x: left + offset.x + shape.rect.x,
    y: top + offset.y + shape.rect.y,
    width: shape.rect.width,
    height: shape.rect.height,
    topRadius: shape.topRadius,
    bottomRadius: shape.bottomRadius,
    ...(hit.ears ? { ears: hit.ears } : {}),
  };
}

/**
 * The island drawn at `size`, on its way to `target`: it answers to the larger of the two.
 * `held` is the size the window may be cut to, which is never more than `size`.
 */
export function surfaceArea(
  geometry: Geometry,
  target: Size,
  size: Size,
  held: Size,
  left: number,
  top = geometry.floatingDrop,
  edge: IslandEdge = "top",
  position?: IslandPosition,
  offsetOf?: OffsetOf,
): Area {
  const reach = { width: Math.max(size.width, target.width), height: Math.max(size.height, target.height) };
  return {
    reach: outlineAt(geometry, target, left, reach, "reach", top, edge, position, offsetOf),
    drawn: outlineAt(geometry, target, left, held, "painted", top, edge, position, offsetOf),
  };
}

/** How long a frame takes to get from the page onto the screen, with some room (measured at two frames). */
export const LAG_MS = 60;

/**
 * The compositor shows a frame a little after the window is cut to it, so an
 * island that grows would be cut to a size the pixels have not reached yet, and
 * the blur behind shows in the difference. Holding the cut to the smallest size
 * of the last `LAG_MS` keeps it inside what is on screen when the island grows,
 * and a shrinking island is cut at once.
 */
export class Trailing {
  private seen: { at: number; size: Size }[] = [];

  /** The size to cut to for a frame of `size` drawn at `at` ms. */
  push(size: Size, at: number): Size {
    this.seen.push({ at, size });
    while (this.seen.length > 1 && this.seen[0].at < at - LAG_MS) this.seen.shift();
    return {
      width: Math.min(...this.seen.map((s) => s.size.width)),
      height: Math.min(...this.seen.map((s) => s.size.height)),
    };
  }

  /** The island has arrived: nothing is behind any more. */
  reset() {
    this.seen = [];
  }
}

/** A round button `scale` of the way to its full size, around the same centre. */
export function circleDrawn(full: HitRect, scale: number): HitRect {
  const width = full.width * scale;
  const height = full.height * scale;
  return {
    x: full.x + (full.width - width) / 2,
    y: full.y + (full.height - height) / 2,
    width,
    height,
    topRadius: full.topRadius * scale,
    bottomRadius: full.bottomRadius * scale,
  };
}

interface Circle {
  id: string;
  reach: HitRect;
}

let surface: Area | null = null;
let transitionOrigin: HitRect | null = null;
let circles: Circle[] = [];
/** How much of its size each button is drawn at now; 0 while it is not on screen. */
const scales = new Map<string, number>();
let published = "";
let queued = false;
let sending = false;

function flush() {
  queued = false;
  const shown = circles.flatMap((c) => {
    const scale = scales.get(c.id) ?? 0;
    return scale > 0 ? [circleDrawn(c.reach, scale)] : [];
  });
  const rects = [
    ...(surface ? [surface.reach, ...(transitionOrigin ? [transitionOrigin] : [])] : []),
    ...circles.map((c) => c.reach),
  ];
  const drawn = [...(surface ? [surface.drawn] : []), ...shown];
  const key = JSON.stringify([rects, drawn]);
  if (key === published || sending) return;
  published = key;
  sending = true;
  void bridge.setHitRects(rects, drawn).finally(() => {
    sending = false;
    schedule();
  });
}

/** Everything that changes in one frame goes out as one message. */
function schedule() {
  if (queued) return;
  queued = true;
  queueMicrotask(flush);
}

export function setSurface(area: Area, settled = true) {
  if (!settled && !transitionOrigin) transitionOrigin = surface?.drawn ?? null;
  if (settled) transitionOrigin = null;
  surface = area;
  schedule();
}

/** The buttons that take the pointer, at their full size. */
export function setCircles(next: Circle[]) {
  circles = next;
  schedule();
}

export function setCircleScale(id: string, scale: number) {
  if ((scales.get(id) ?? 0) === scale) return;
  scales.set(id, scale);
  schedule();
}

/** Nothing is drawn and nothing takes the pointer. */
export function clearAreas() {
  surface = null;
  transitionOrigin = null;
  circles = [];
  scales.clear();
  published = "";
  flush();
}
