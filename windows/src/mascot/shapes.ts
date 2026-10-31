// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The companion's outlines in a square of `size` pixels, from its top left
// corner, as SVG paths (NotchMascotGeometry). Every eye is drawn with the same
// number of curves, so any eye can turn into any other one point at a time.

import type { MascotEye, MascotFace, MascotLook, MascotShape } from "./model";

export const EYE_CURVES = 24;

interface Point {
  x: number;
  y: number;
}

const n = (value: number) => Number(value.toFixed(3));

function roundedRect(x: number, y: number, width: number, height: number, radius: number): string {
  const r = Math.min(radius, width / 2, height / 2);
  return (
    `M${n(x + r)} ${n(y)}H${n(x + width - r)}A${n(r)} ${n(r)} 0 0 1 ${n(x + width)} ${n(y + r)}` +
    `V${n(y + height - r)}A${n(r)} ${n(r)} 0 0 1 ${n(x + width - r)} ${n(y + height)}` +
    `H${n(x + r)}A${n(r)} ${n(r)} 0 0 1 ${n(x)} ${n(y + height - r)}` +
    `V${n(y + r)}A${n(r)} ${n(r)} 0 0 1 ${n(x + r)} ${n(y)}Z`
  );
}

function ellipse(x: number, y: number, width: number, height: number): string {
  const rx = width / 2;
  const ry = height / 2;
  return (
    `M${n(x)} ${n(y + ry)}A${n(rx)} ${n(ry)} 0 1 1 ${n(x + width)} ${n(y + ry)}` +
    `A${n(rx)} ${n(ry)} 0 1 1 ${n(x)} ${n(y + ry)}Z`
  );
}

export function bodyPath(shape: MascotShape, size: number): string {
  const p = (x: number, y: number) => `${n(x * size)} ${n(y * size)}`;
  switch (shape) {
    case "ball":
      return ellipse(0.08 * size, 0.12 * size, 0.84 * size, 0.84 * size);
    case "egg":
      return (
        `M${p(0.5, 0.1)}C${p(0.71, 0.1)} ${p(0.86, 0.36)} ${p(0.86, 0.6)}` +
        `C${p(0.86, 0.8)} ${p(0.7, 0.96)} ${p(0.5, 0.96)}` +
        `C${p(0.3, 0.96)} ${p(0.14, 0.8)} ${p(0.14, 0.6)}` +
        `C${p(0.14, 0.36)} ${p(0.29, 0.1)} ${p(0.5, 0.1)}Z`
      );
    case "squircle": {
      // A superellipse: flatter sides than a rounded rectangle, no corner seam.
      const count = 72;
      const exponent = 2 / 3.4;
      const points: string[] = [];
      for (let index = 0; index < count; index++) {
        const angle = (index / count) * 2 * Math.PI;
        const cosine = Math.cos(angle);
        const sine = Math.sin(angle);
        const x = 0.5 + 0.43 * (cosine < 0 ? -1 : 1) * Math.abs(cosine) ** exponent;
        const y = 0.55 + 0.4 * (sine < 0 ? -1 : 1) * Math.abs(sine) ** exponent;
        points.push(p(x, y));
      }
      return `M${points.join("L")}Z`;
    }
    case "pill":
      return roundedRect(0.05 * size, 0.28 * size, 0.9 * size, 0.6 * size, 0.3 * size);
  }
}

export interface RobotPaths {
  head: string;
  ears: string;
  antenna: string;
  bulb: string;
  visor: string;
}

export function robotPaths(size: number): RobotPaths {
  const rect = (x: number, y: number, width: number, height: number, radius: number) =>
    roundedRect(x * size, y * size, width * size, height * size, radius * size);
  return {
    head: rect(0.12, 0.26, 0.76, 0.68, 0.2),
    ears: rect(0.05, 0.5, 0.08, 0.2, 0.035) + rect(0.87, 0.5, 0.08, 0.2, 0.035),
    antenna: rect(0.475, 0.12, 0.05, 0.16, 0.025),
    bulb: ellipse(0.43 * size, 0.04 * size, 0.14 * size, 0.14 * size),
    visor: rect(0.2, 0.44, 0.6, 0.38, 0.15),
  };
}

/** The highlight, an ellipse leaning with the body's curve. */
export function shineTransform(face: MascotFace, size: number): string {
  return `translate(${n(face.shine.x * size)} ${n(face.shine.y * size)}) rotate(${n((face.shine.angle * 180) / Math.PI)})`;
}

const FIGURE_MIDDLE: Record<MascotShape, number> = { ball: 0.54, egg: 0.53, squircle: 0.55, pill: 0.58 };

/**
 * How far below the middle of its box the figure's own middle sits, as a share
 * of the box. The box keeps room above a minimal body for the robot's antenna,
 * so a body placed by its box would sit a little low.
 */
export function figureOffset(look: Pick<MascotLook, "style" | "shape">): number {
  return (look.style === "robot" ? 0.49 : FIGURE_MIDDLE[look.shape]) - 0.5;
}

// MARK: Eyes

const DENSE = 96;

function cornerPoints(left: number, top: number, width: number, height: number, radius: number): Point[] {
  const right = left + width;
  const bottom = top + height;
  const corners: [number, number, number][] = [
    [right - radius, top + radius, -Math.PI / 2],
    [right - radius, bottom - radius, 0],
    [left + radius, bottom - radius, Math.PI / 2],
    [left + radius, top + radius, Math.PI],
  ];
  const steps = DENSE / 4;
  const points: Point[] = [];
  for (const [x, y, start] of corners) {
    for (let step = 0; step <= steps; step++) {
      const angle = start + (step / steps) * (Math.PI / 2);
      points.push({ x: x + radius * Math.cos(angle), y: y + radius * Math.sin(angle) });
    }
  }
  return points;
}

/** A capsule standing on its round ends, or lying on them when wider. */
function stadium(width: number, height: number, center: Point): Point[] {
  return cornerPoints(center.x - width / 2, center.y - height / 2, width, height, Math.min(width, height) / 2);
}

function ellipsePoints(rx: number, ry: number, center: Point): Point[] {
  return Array.from({ length: DENSE }, (_, index) => {
    const angle = -Math.PI / 2 + (index / DENSE) * 2 * Math.PI;
    return { x: center.x + rx * Math.cos(angle), y: center.y + ry * Math.sin(angle) };
  });
}

/** Eyes shut with a smile: a band bent upward, round at both ends. */
function arch(span: number, rise: number, thickness: number, top: number): Point[] {
  const half = Math.max(0.1, span / 2 - thickness / 2);
  const lift = Math.max(0.1, rise);
  const radius = (half * half + lift * lift) / (2 * lift);
  const center = { x: 0, y: top + thickness / 2 + radius };
  const reach = Math.asin(Math.min(1, half / radius));
  const onArc = (angle: number, r: number) => ({
    x: center.x + r * Math.sin(angle),
    y: center.y - r * Math.cos(angle),
  });
  const steps = Math.floor(DENSE / 3);
  const points: Point[] = [];
  for (let step = 0; step <= steps; step++) {
    points.push(onArc(-reach + (step / steps) * reach * 2, radius + thickness / 2));
  }
  // The right end's round cap, from the outer arc to the inner one.
  const rightEnd = onArc(reach, radius);
  for (let step = 1; step < 12; step++) {
    const angle = reach + (step / 12) * Math.PI;
    points.push({
      x: rightEnd.x + (thickness / 2) * Math.sin(angle),
      y: rightEnd.y - (thickness / 2) * Math.cos(angle),
    });
  }
  for (let step = 0; step <= steps; step++) {
    points.push(onArc(reach - (step / steps) * reach * 2, radius - thickness / 2));
  }
  const leftEnd = onArc(-reach, radius);
  for (let step = 1; step < 12; step++) {
    const angle = -reach + Math.PI + (step / 12) * Math.PI;
    points.push({
      x: leftEnd.x + (thickness / 2) * Math.sin(angle),
      y: leftEnd.y - (thickness / 2) * Math.cos(angle),
    });
  }
  return points;
}

function heart(width: number, height: number, center: Point): Point[] {
  // The classic heart curve, 32 wide and about 29 tall, with y up.
  const raw = Array.from({ length: DENSE }, (_, index) => {
    const t = (index / DENSE) * 2 * Math.PI;
    return {
      x: 16 * Math.sin(t) ** 3,
      y: 13 * Math.cos(t) - 5 * Math.cos(2 * t) - 2 * Math.cos(3 * t) - Math.cos(4 * t),
    };
  });
  const ys = raw.map((point) => point.y);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const middle = (minY + maxY) / 2;
  return raw.map((point) => ({
    x: center.x + (point.x / 32) * width,
    y: center.y - ((point.y - middle) / (maxY - minY)) * height,
  }));
}

/** A drowsy eye: the lid's flat edge on top, the eye's round bottom below. */
function lid(width: number, top: number, depth: number): Point[] {
  const half = width / 2;
  const flat = DENSE / 4;
  const round = DENSE - flat;
  const points: Point[] = [];
  for (let step = 0; step <= flat; step++) points.push({ x: -half + (step / flat) * width, y: top });
  for (let step = 1; step < round; step++) {
    const angle = (step / round) * Math.PI;
    points.push({ x: half * Math.cos(angle), y: top + depth * Math.sin(angle) });
  }
  return points;
}

/** The part of a convex outline below the line from `from` to `to`. */
function clipBelow(points: Point[], from: Point, to: Point): Point[] {
  const side = (point: Point) => (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x);
  const clipped: Point[] = [];
  points.forEach((current, index) => {
    const next = points[(index + 1) % points.length]!;
    const a = side(current);
    const b = side(next);
    if (a >= 0) clipped.push(current);
    if (a >= 0 !== b >= 0) {
      const share = a / (a - b);
      clipped.push({ x: current.x + (next.x - current.x) * share, y: current.y + (next.y - current.y) * share });
    }
  });
  return clipped.length >= 3 ? clipped : points;
}

/**
 * `count` points evenly spaced along the outline, starting where its top crosses
 * the middle line and going clockwise on screen. Every eye then has its points
 * in the same places, which is what lets one become another.
 */
export function resample(outline: Point[], count: number): Point[] {
  if (outline.length < 3 || count < 3) return outline;
  let points = outline;
  // Clockwise on screen, with y growing downward, is a positive area.
  let area = 0;
  points.forEach((a, index) => {
    const b = points[(index + 1) % points.length]!;
    area += a.x * b.y - b.x * a.y;
  });
  if (area < 0) points = [...points].reverse();
  // The start: the highest place where the outline crosses x = 0.
  let start: { index: number; point: Point } | null = null;
  points.forEach((a, index) => {
    const b = points[(index + 1) % points.length]!;
    if (!((a.x <= 0 && b.x > 0) || (a.x >= 0 && b.x < 0) || a.x === 0)) return;
    const share = b.x === a.x ? 0 : -a.x / (b.x - a.x);
    const crossing = { x: 0, y: a.y + (b.y - a.y) * share };
    if (!start || crossing.y < start.point.y) start = { index, point: crossing };
  });
  if (!start) return points.slice(0, count);
  const { index: first, point: crossing } = start as { index: number; point: Point };
  const walk = [crossing];
  for (let offset = 1; offset <= points.length; offset++) walk.push(points[(first + offset) % points.length]!);
  walk.push(crossing);
  const lengths = [0];
  for (let index = 1; index < walk.length; index++) {
    lengths.push(
      lengths[index - 1]! + Math.hypot(walk[index]!.x - walk[index - 1]!.x, walk[index]!.y - walk[index - 1]!.y),
    );
  }
  const total = lengths[lengths.length - 1]!;
  if (total <= 0) return Array.from({ length: count }, () => crossing);
  const result: Point[] = [];
  let segment = 1;
  for (let step = 0; step < count; step++) {
    const target = (total * step) / count;
    while (segment < lengths.length - 1 && lengths[segment]! < target) segment++;
    const span = lengths[segment]! - lengths[segment - 1]!;
    const share = span > 0 ? (target - lengths[segment - 1]!) / span : 0;
    const a = walk[segment - 1]!;
    const b = walk[segment]!;
    result.push({ x: a.x + (b.x - a.x) * share, y: a.y + (b.y - a.y) * share });
  }
  return result;
}

/** The eye's outline as `EYE_CURVES` points around (0, 0). `width` and `height` are an open eye's. */
export function eyePoints(eye: MascotEye, width: number, height: number, right: boolean): Point[] {
  const w = Math.max(0.5, width);
  const h = Math.max(0.5, height);
  let outline: Point[];
  switch (eye) {
    case "open":
      outline = stadium(w, h, { x: 0, y: 0 });
      break;
    case "closed":
      outline = stadium(w * 1.5, w * 0.45, { x: 0, y: h * 0.12 });
      break;
    case "squint":
      outline = stadium(w, h * 0.55, { x: 0, y: h * 0.08 });
      break;
    case "wide":
      outline = ellipsePoints(w * 0.8, w * 0.8, { x: 0, y: -h * 0.02 });
      break;
    case "happy":
      outline = arch(w * 2.1, h * 0.42, w * 0.55, -h * 0.18);
      break;
    case "heart":
      outline = heart(w * 2.0, w * 1.8, { x: 0, y: -h * 0.02 });
      break;
    case "sleepy":
      outline = lid(w * 1.25, h * 0.02, h * 0.38);
      break;
    case "determined":
      // The left eye's lid falls toward the nose, on its right.
      outline = clipBelow(stadium(w, h, { x: 0, y: 0 }), { x: -w / 2, y: -h * 0.3 }, { x: w / 2, y: h * 0.02 });
      break;
  }
  if (right) outline = outline.map((point) => ({ x: -point.x, y: point.y }));
  return resample(outline, EYE_CURVES);
}

/** A closed curve through every point, one cubic per point. */
export function smoothClosedPath(points: Point[]): string {
  const count = points.length;
  if (count < 3) return "";
  let d = `M${n(points[0]!.x)} ${n(points[0]!.y)}`;
  for (let index = 0; index < count; index++) {
    const previous = points[(index - 1 + count) % count]!;
    const current = points[index]!;
    const next = points[(index + 1) % count]!;
    const after = points[(index + 2) % count]!;
    d +=
      `C${n(current.x + (next.x - previous.x) / 6)} ${n(current.y + (next.y - previous.y) / 6)} ` +
      `${n(next.x - (after.x - current.x) / 6)} ${n(next.y - (after.y - current.y) / 6)} ` +
      `${n(next.x)} ${n(next.y)}`;
  }
  return `${d}Z`;
}

/** An eye's path around `center`. */
export function eyePath(eye: MascotEye, width: number, height: number, center: Point, right: boolean): string {
  return smoothClosedPath(
    eyePoints(eye, width, height, right).map((point) => ({ x: point.x + center.x, y: point.y + center.y })),
  );
}
