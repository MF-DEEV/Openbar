// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's sizes and outlines on a display without a camera cutout,
// ported from NotchGeometry / NotchLayout (NotchSupport.swift) and NotchShape
// (NotchView.swift). One point here is one CSS pixel (one DIP on Windows).
// See windows/docs/spec/01-shell.md §1 for every formula and its source line.

import type { Size } from "./motion";

export type Silhouette = "notch" | "capsule";
export type LayoutPreset = "compact" | "spacious" | "custom";

/** Bézier constant for a quarter circle. */
const K = 0.55228475;
const SHOULDER = 14;
/** The simulated camera on a display without one: 180 · 24 / 32. */
const SIMULATED_CAMERA = 135;
const MINIMUM_WING = 44;
const QUICK_ACCESS_GUTTER = 72;
const HORIZONTAL_INSET = 28;
const BOTTOM_INSET = 16;
const HEADER_ROW = 36;

export const surfaceRadius = (h: number) => Math.min(28, h * 0.34);
export const shoulder = (h: number) => Math.min(SHOULDER, h * 0.19);
export const capsuleRadius = (h: number) => Math.min(Math.max(0, h) / 2, 28);

export interface GeometryInput {
  /** The display, in DIPs. */
  screen: Size;
  devicePixelRatio: number;
  silhouette: Silhouette;
  layout: LayoutPreset;
  customWidth?: number;
  customHeight?: number;
  notchWidth?: number;
  notchHeight?: number;
  roundness?: number;
  shoulderWidth?: number;
  shoulderHeight?: number;
  /** Capsule fit: width −40…40, height −4…4, drop 0…20. */
  capsuleFit?: { width: number; height: number; drop: number };
  /** Height of the bar the island sits in (the Windows menu bar imitation). */
  barHeight?: number;
}

export interface Geometry {
  input: GeometryInput;
  screen: Size;
  silhouette: Silhouette;
  /** Capsule only: how far the capsule floats below the top edge. */
  floatingGap: number | null;
  floatingDrop: number;
  cameraWidth: number;
  stripHeight: number;
  menuBarHeight: number;
  /** Where content may start below the strip. */
  safeContentTop: number;
  /** Free room either side of the camera in an empty bar. */
  compactSideRoom: number;
}

export function makeGeometry(input: GeometryInput): Geometry {
  const bar = input.barHeight ?? 24;
  const edge = Math.min(1, Math.max(0, 1 / input.devicePixelRatio));
  const fit = input.capsuleFit ?? { width: 0, height: 0, drop: 0 };
  const capsule = input.silhouette === "capsule";
  const gap = capsule ? Math.min(2, Math.max(0, Math.floor((bar - 20) / 2))) : null;
  const strip =
    gap !== null
      ? Math.max(bar - edge + fit.height, 2 * gap + 12)
      : Math.min(64, Math.max(20, input.notchHeight ?? bar));
  const camera = Math.min(
    gap !== null
      ? Math.round((180 * bar) / 32 + fit.width)
      : Math.min(400, Math.max(100, input.notchWidth ?? SIMULATED_CAMERA)),
    input.screen.width * 0.7,
  );
  return {
    input,
    screen: input.screen,
    silhouette: input.silhouette,
    floatingGap: gap,
    floatingDrop: gap !== null ? fit.drop : 0,
    cameraWidth: camera,
    stripHeight: strip,
    menuBarHeight: Math.max(strip, bar),
    safeContentTop: gap !== null ? BOTTOM_INSET : strip + 10,
    compactSideRoom: (input.screen.width - camera) / 2 - 8,
  };
}

// MARK: Outlines

/** SVG path of the hanging island ("notch"), top edge spanning the ears. */
type Contour = Pick<GeometryInput, "roundness" | "shoulderWidth" | "shoulderHeight">;
const factor = (value = 100, max = 300) => Math.min(max, Math.max(0, value)) / 100;

/** Shared by drawing, hit testing and content insets. */
export function notchMetrics(width: number, height: number, contour: Contour = {}) {
  const w = Math.max(0, width);
  const h = Math.max(0, height);
  const s = Math.min(shoulder(h) * factor(contour.shoulderWidth), w / 2);
  const rise = Math.min(shoulder(h) * factor(contour.shoulderHeight), h / 2);
  const b = Math.max(0, Math.min(surfaceRadius(h) * factor(contour.roundness, 200), h / 2, (w - s * 2) / 2));
  return { s, rise, b };
}

export function notchPath(width: number, height: number, contour: Contour = {}): string {
  const w = Math.max(0, width);
  const h = Math.max(0, height);
  const { s, rise, b } = notchMetrics(w, h, contour);
  const t = 1 - K;
  return [
    `M0 0`,
    `L${w} 0`,
    `C${w - s * K} 0 ${w - s} ${rise * t} ${w - s} ${rise}`,
    `L${w - s} ${h - b}`,
    `C${w - s} ${h - b * t} ${w - s - b * t} ${h} ${w - s - b} ${h}`,
    `L${s + b} ${h}`,
    `C${s + b * t} ${h} ${s} ${h - b * t} ${s} ${h - b}`,
    `L${s} ${rise}`,
    `C${s} ${rise * t} ${s * K} 0 0 0`,
    `Z`,
  ].join(" ");
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The capsule inside a surface: `gap` in from top and bottom, shoulders in from the sides. */
export function capsuleBody(width: number, height: number, gap: number): Rect {
  const w = Math.max(0, width);
  const h = Math.max(0, height);
  const side = Math.min(shoulder(h), w / 2);
  const inset = Math.min(Math.max(0, gap), h / 2);
  return { x: side, y: inset, width: w - side * 2, height: h - inset * 2 };
}

/** SVG path of the floating capsule; every size keeps the same elements so frames blend. */
export function capsulePath(width: number, height: number, gap: number, roundness = 100): string {
  const body = capsuleBody(width, height, gap);
  const c = Math.min(capsuleRadius(body.height) * factor(roundness, 200), body.height / 2, body.width / 2);
  const hd = c * (1 - K);
  const l = body.x;
  const r = body.x + body.width;
  const t = body.y;
  const b = body.y + body.height;
  return [
    `M${l + c} ${t}`,
    `L${r - c} ${t}`,
    `C${r - hd} ${t} ${r} ${t + hd} ${r} ${t + c}`,
    `L${r} ${b - c}`,
    `C${r} ${b - hd} ${r - hd} ${b} ${r - c} ${b}`,
    `L${l + c} ${b}`,
    `C${l + hd} ${b} ${l} ${b - hd} ${l} ${b - c}`,
    `L${l} ${t + c}`,
    `C${l} ${t + hd} ${l + hd} ${t} ${l + c} ${t}`,
    `Z`,
  ].join(" ");
}

export function islandPath(geometry: Geometry, size: Size): string {
  return geometry.floatingGap !== null
    ? capsulePath(size.width, size.height, geometry.floatingGap, geometry.input.roundness)
    : notchPath(size.width, size.height, geometry.input);
}

/**
 * The area that takes the pointer, as rounded rects for the pointer watcher:
 * the visible body plus, for a capsule, the strip above it up to the screen
 * edge, so a pointer pushed against the top still hits the island.
 */
export function hitShape(geometry: Geometry, size: Size): { rect: Rect; topRadius: number; bottomRadius: number } {
  if (geometry.floatingGap !== null) {
    const body = capsuleBody(size.width, size.height, geometry.floatingGap);
    const c = Math.min(
      capsuleRadius(body.height) * factor(geometry.input.roundness, 200),
      body.height / 2,
      body.width / 2,
    );
    return {
      rect: { x: body.x, y: 0, width: body.width, height: body.y + body.height },
      topRadius: 0,
      bottomRadius: c,
    };
  }
  const { s, b } = notchMetrics(size.width, size.height, geometry.input);
  return {
    rect: { x: s, y: 0, width: Math.max(0, size.width - s * 2), height: size.height },
    topRadius: 0,
    bottomRadius: b,
  };
}

/** Keep content inside the chosen contour, including its bottom corners. */
export function contentInsets(geometry: Geometry, size: Size) {
  const body = geometry.floatingGap !== null ? capsuleBody(size.width, size.height, geometry.floatingGap) : null;
  const notch = notchMetrics(size.width, size.height, geometry.input);
  const corner = body
    ? Math.min(capsuleRadius(body.height) * factor(geometry.input.roundness, 200), body.height / 2, body.width / 2)
    : notch.b;
  const side = body?.x ?? notch.s;
  return {
    horizontal: Math.min(size.width / 2, Math.max(HORIZONTAL_INSET, side + Math.max(12, corner / 2))),
    bottom: Math.max(BOTTOM_INSET, (geometry.floatingGap ?? 0) + corner / 2),
  };
}

// MARK: Sizes

/** The closed island with nothing to show. */
export function restingSize(geometry: Geometry, showsIdleContent = false): Size {
  const strip = geometry.stripHeight;
  if (geometry.floatingGap !== null) {
    const body = strip - 2 * geometry.floatingGap;
    const fitWidth = geometry.input.capsuleFit?.width ?? 0;
    const width = Math.min(
      geometry.cameraWidth,
      Math.round(Math.max(5 * body + fitWidth, 2 * body) + 2 * shoulder(strip)),
    );
    return { width, height: strip };
  }
  const wing = showsIdleContent ? restingWingWidth(geometry) : 0;
  return { width: Math.min(geometry.screen.width - 24, geometry.cameraWidth + 2 * wing), height: strip };
}

/** The wing beside the camera the closed island opens for what rests there; none when the display has no room. */
export const restingWingWidth = (geometry: Geometry) => (geometry.compactSideRoom >= MINIMUM_WING ? MINIMUM_WING : 0);

/** A subtle, proportional hover response; choices remain a click action. */
export function hoverPulse(geometry: Geometry, closed: Size): Size {
  const scale = Math.max(1, Math.min(1.03, (geometry.screen.width - 24) / closed.width));
  return {
    width: closed.width * scale,
    height: closed.height * scale,
  };
}

export function peekSize(geometry: Geometry): Size {
  return {
    width: Math.min(geometry.screen.width - 24, Math.max(geometry.cameraWidth + 110, 340)),
    height: geometry.safeContentTop + 52,
  };
}

export function preferredWidth(geometry: Geometry): number {
  switch (geometry.input.layout) {
    case "compact":
      return 480;
    case "spacious":
      return 560;
    case "custom":
      return geometry.input.customWidth ?? 440;
  }
}

/** Width of the open island; `natural` is a page's own width (music 344, timer 344, controls). */
export function expandedWidth(geometry: Geometry, opts: { natural?: number; enlarged?: boolean } = {}): number {
  const limit = geometry.screen.width - 24 - 2 * QUICK_ACCESS_GUTTER;
  const min = geometry.cameraWidth + 36;
  if (opts.enlarged) {
    return Math.min(Math.max(Math.min(1180, 0.66 * geometry.screen.width), min), limit);
  }
  const chosen = preferredWidth(geometry);
  const preferred =
    geometry.input.layout !== "custom" && opts.natural !== undefined ? Math.min(chosen, opts.natural + 56) : chosen;
  return Math.min(Math.max(preferred, min), limit);
}

export interface HeaderMetrics {
  headerTopInset: number;
  headerRowHeight: number;
  pageTopInset: number;
  headerChromeHeight: number;
}

export function headerMetrics(geometry: Geometry, hasRow: boolean): HeaderMetrics {
  const row = hasRow ? HEADER_ROW : 0;
  const top = row === 0 ? 16 : geometry.floatingGap !== null ? 16 - (HEADER_ROW - 28) / 2 : 0;
  const pageTop = row > 0 ? top + row + 12 : top;
  return {
    headerTopInset: top,
    headerRowHeight: row,
    pageTopInset: pageTop,
    headerChromeHeight: pageTop - top + 16,
  };
}

export function contentBudget(geometry: Geometry, hasRow: boolean): number {
  switch (geometry.input.layout) {
    case "compact":
      return 180;
    case "spacious":
      return 264;
    case "custom": {
      const h = headerMetrics(geometry, hasRow);
      return (geometry.input.customHeight ?? 480) - h.headerTopInset - h.headerChromeHeight;
    }
  }
}

/**
 * Height of the open island for a page whose content is `content` tall (or
 * the preset's budget when omitted), capped to the display.
 */
export function expandedHeight(
  geometry: Geometry,
  opts: { hasRow: boolean; content?: number; enlarged?: boolean; bottomBubbles?: boolean },
): number {
  const cap = geometry.screen.height - 48 - (opts.bottomBubbles ? QUICK_ACCESS_GUTTER : 0);
  if (opts.enlarged) return Math.min(0.8 * geometry.screen.height, geometry.screen.height - 48 - 72);
  const h = headerMetrics(geometry, opts.hasRow);
  let height = h.headerTopInset + h.headerChromeHeight + (opts.content ?? contentBudget(geometry, opts.hasRow));
  if (geometry.input.layout === "custom") height = geometry.input.customHeight ?? 480;
  return Math.min(height, cap);
}

/** Fit a complete logical layout without stretching either axis or its children. */
export function fitContent(available: Size, content: Size) {
  const scale = Math.max(0, Math.min(available.width / content.width, available.height / content.height));
  return {
    scale,
    x: (available.width - content.width * scale) / 2,
    y: (available.height - content.height * scale) / 2,
  };
}

export const Layout = {
  horizontalInset: HORIZONTAL_INSET,
  bottomInset: BOTTOM_INSET,
  headerRow: HEADER_ROW,
  quickAccessGutter: QUICK_ACCESS_GUTTER,
  minimumWing: MINIMUM_WING,
} as const;

/** A banner opened under the pointer (NotchGeometry.notificationPreviewSize). */
export function notificationPreviewSize(geometry: Geometry, contentHeight: number): Size {
  const width = Math.min(Math.max(400, geometry.cameraWidth + 200), expandedWidth(geometry));
  const height = geometry.safeContentTop + contentHeight;
  const { bottom } = contentInsets(geometry, { width, height: height + BOTTOM_INSET });
  return { width, height: Math.min(height + bottom, geometry.screen.height - 48) };
}

// MARK: Live activities

/** Breathing room every compact strip keeps from its silhouette. */
export const COMPACT_EDGE_GAP = 5;

/** A closed island showing a live activity: wings of `wing` either side of the camera (capsule: one row). */
export function activitySize(geometry: Geometry, wing: number, capsuleContent?: number): Size {
  if (geometry.floatingGap !== null) {
    const rest = restingSize(geometry);
    const side = shoulder(geometry.stripHeight);
    const available = Math.min(geometry.screen.width - 24, geometry.cameraWidth + 2 * geometry.compactSideRoom);
    const content = capsuleContent ?? 2 * wing;
    return {
      width: Math.min(Math.max(Math.ceil(content + 8 + 8 + 2 * side), rest.width), available),
      height: geometry.stripHeight,
    };
  }
  const extraInset = Math.max(
    0,
    notchMetrics(1000, geometry.stripHeight, geometry.input).s - shoulder(geometry.stripHeight),
  );
  return {
    width: Math.min(geometry.screen.width - 24, geometry.cameraWidth + 2 * (wing + extraInset)),
    height: geometry.stripHeight,
  };
}

/** The Mac's compact chooser: a stable strip above up to three named choices per row. */
export function activityPickerLayout(geometry: Geometry, strips: Size[]) {
  const limit = Math.max(1, geometry.screen.width - 24);
  const insets = contentInsets(geometry, { width: limit, height: 200 });
  const horizontal = Math.max(24, insets.horizontal);
  const columns = Math.min(3, Math.max(1, strips.length), Math.max(1, Math.floor((limit - 2 * horizontal + 6) / 118)));
  const rows = Math.ceil(Math.max(1, strips.length) / columns);
  const headerHeight = Math.max(geometry.stripHeight, ...strips.map((size) => size.height));
  return {
    columns,
    headerHeight,
    horizontal,
    size: {
      width: Math.min(
        limit,
        Math.max(...strips.map((size) => size.width), columns * 112 + (columns - 1) * 6) + 2 * horizontal,
      ),
      height: headerHeight + 12 + rows * 32 + (rows - 1) * 6 + Math.max(12, insets.bottom),
    },
  };
}

/**
 * How far in from the strip's end a vertically centred box of `boxHeight`,
 * rounded by `radius`, sits an even `gap` from the silhouette's bottom
 * corner (NotchGeometry.compactActivityEdgeInset).
 */
export function edgeInset(
  size: Size,
  boxHeight: number,
  radius: number,
  gap = COMPACT_EDGE_GAP,
  contour: Contour = {},
): number {
  const { s, b: corner } = notchMetrics(size.width, size.height, contour);
  const flat = s + gap;
  const below = (size.height - boxHeight) / 2;
  const reach = corner - radius - gap;
  const drop = corner - radius - below;
  if (reach <= 0 || drop <= 0) return Math.max(0, flat);
  const span = reach > drop ? Math.sqrt(reach * reach - drop * drop) : 0;
  return Math.max(0, flat, s + corner - radius - span);
}
