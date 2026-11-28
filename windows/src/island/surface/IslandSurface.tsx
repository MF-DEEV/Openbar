// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Draws the island's outline frame by frame from the resize timeline and
// clips the content to it (NotchWindowHost.swift; spec 01-shell §3.1). The
// expanded content sits at its destination size; compact content follows
// the visible surface. Each frame also tells Rust which part of the
// window takes the pointer.

import { useLocale } from "../../i18n";

import { type ReactNode, useId, useLayoutEffect, useRef, useState } from "react";
import { useIsland, visibleActivity } from "../store";
import { type Geometry, edgePath } from "../geometry";
import { MotionTimeline, type Size } from "../motion";
import { DrawnOffset, DrawnSize } from "./DrawnSize";
import { setSurface, surfaceArea, Trailing } from "./hits";
import { surfaceOffset, type IslandEdge, type IslandPosition } from "../placement";

const now = () => performance.now() / 1000;

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** Path `d` turned onto its edge, with its origin moved by `dx`, `dy` (path() has no transform of its own). */
function shifted(geometry: Geometry, size: Size, dx: number, dy: number, edge: IslandEdge): string {
  const d = edgePath(geometry, size, edge);
  if (dx === 0 && dy === 0) return d;
  return d.replace(
    /(-?\d*\.?\d+(?:e-?\d+)?) (-?\d*\.?\d+(?:e-?\d+)?)/g,
    (_, x, y) => `${Number(x) + dx} ${Number(y) + dy}`,
  );
}

export function IslandSurface(props: {
  geometry: Geometry;
  /** What is drawn when it differs from what lays out the content (a notch on a side edge). */
  outline?: Geometry;
  target: Size;
  left?: number;
  top?: number;
  edge?: IslandEdge;
  position?: IslandPosition;
  smoothPosition?: boolean;
  onDrawn?: (size: Size, settled: boolean) => void;
  children: ReactNode;
}) {
  useLocale();
  const { geometry, target, children } = props;
  const shape = props.outline ?? geometry;
  const outline = useIsland((s) => s.settings.outline);
  // The outline turns system orange while the timer has the closed island (NotchService.swift:2837-2838).
  const timerShown = useIsland(
    (s) => !s.expanded && !s.notice && !s.peeking && !s.dropHint && visibleActivity(s) === "timer",
  );
  const clipId = `island-clip-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const frameRef = useRef<HTMLDivElement>(null);
  const drawn = useRef<Size>(target);
  const [contentSize, setContentSize] = useState<Size>(target);
  const timeline = useRef<MotionTimeline | null>(null);
  const raf = useRef(0);
  const trailing = useRef(new Trailing());
  const onDrawn = useRef(props.onDrawn);
  onDrawn.current = props.onDrawn;

  // Along its edge the outline travels from where it was drawn when the
  // target changed to where the new frame puts it, in step with its size.
  // A fixed anchor would jump wherever the open panel is pushed in from a
  // screen corner and the closed island is not.
  const edge = props.edge ?? "top";
  const horizontal = edge === "top" || edge === "bottom";
  const frameStart = horizontal ? props.left : props.top;
  const travelKey = `${edge}:${target.width}x${target.height}@${props.left},${props.top}`;
  const last = useRef<{ edge: IslandEdge; start: number; size: number } | null>(null);
  const travel = useRef<{ key: string; start: number; size: number } | null>(null);
  if (travel.current?.key !== travelKey) {
    const from = last.current?.edge === edge ? last.current : null;
    travel.current = { key: travelKey, start: from?.start ?? NaN, size: from?.size ?? NaN };
  }
  const offsetOf = (size: Size) => {
    const base = surfaceOffset(target, size, edge, props.position);
    const from = travel.current;
    if (frameStart === undefined || !from || !Number.isFinite(from.start)) return base;
    const span = (horizontal ? target.width : target.height) - from.size;
    const progress = Math.abs(span) < 0.5 ? 1 : ((horizontal ? size.width : size.height) - from.size) / span;
    const start = from.start + (frameStart - from.start) * progress;
    return horizontal ? { x: start - frameStart, y: base.y } : { x: base.x, y: start - frameStart };
  };

  useLayoutEffect(() => {
    const draw = (size: Size, settled: boolean) => {
      drawn.current = size;
      setContentSize((previous) =>
        previous.width === size.width && previous.height === size.height ? previous : size,
      );
      reportHits(size, settled);
      onDrawn.current?.(size, settled);
    };

    const reportHits = (size: Size, settled: boolean) => {
      const frame = frameRef.current;
      if (!frame) return;
      // Hover and clicks answer to the larger of what is drawn and where it is going;
      // the window is cut to what the screen has caught up with.
      const held = settled ? size : trailing.current.push(size, now() * 1000);
      if (settled) trailing.current.reset();
      const bounds = frame.getBoundingClientRect();
      setSurface(
        surfaceArea(shape, target, size, held, bounds.left, bounds.top, props.edge, props.position, offsetOf),
        settled,
      );
    };

    const t = now();
    const current = timeline.current;
    // Sample the running spring at the retarget time instead of restarting
    // from the previous animation frame when the pointer changes direction.
    const from = current ? current.sizeAt(t) : drawn.current;
    if (reducedMotion() || (from.width === target.width && from.height === target.height && !current)) {
      timeline.current = null;
      draw(target, true);
      if (reducedMotion()) return;
      // Position can move while the outline keeps the same size. Follow the
      // CSS transition so the native pointer region stays on the painted island.
      let followRaf = 0;
      const started = performance.now();
      const follow = () => {
        reportHits(target, true);
        if (performance.now() - started < 220) followRaf = requestAnimationFrame(follow);
      };
      followRaf = requestAnimationFrame(follow);
      return () => cancelAnimationFrame(followRaf);
    }
    timeline.current =
      (current && !current.settledAt(t) ? current.continued(target, t) : null) ??
      MotionTimeline.create(from, target, t);

    cancelAnimationFrame(raf.current);
    const step = () => {
      const tl = timeline.current;
      if (!tl) return;
      const time = now();
      if (tl.settledAt(time)) {
        timeline.current = null;
        draw(tl.target, true);
        return;
      }
      draw(tl.sizeAt(time), false);
      raf.current = requestAnimationFrame(step);
    };
    step();
    return () => cancelAnimationFrame(raf.current);
  }, [shape, target.width, target.height, props.left, props.top, props.edge, props.position?.x, props.position?.y]);

  // Commit the outline, clip and content layout together. Imperatively
  // moving the outline ahead of React's content update causes visible jumps.
  const offset = offsetOf(contentSize);
  const d = shifted(shape, contentSize, offset.x, offset.y, edge);
  useLayoutEffect(() => {
    if (frameStart === undefined) return;
    last.current = {
      edge,
      start: frameStart + (horizontal ? offset.x : offset.y),
      size: horizontal ? contentSize.width : contentSize.height,
    };
  });

  return (
    <div
      ref={frameRef}
      className={`island-frame${props.smoothPosition ? " is-position-moving" : ""}`}
      style={{
        width: target.width,
        height: target.height,
        left: props.left,
        top: props.top ?? geometry.floatingDrop,
        transform: props.left === undefined ? undefined : "none",
      }}
    >
      <svg className="island-backdrop" width={target.width} height={target.height} aria-hidden>
        <clipPath id={clipId}>
          <use href={`#${clipId}-shape`} />
        </clipPath>
        <path id={`${clipId}-shape`} d={d} fill="#000" stroke="#000" strokeWidth={2} />
        {/* Settings › Layout › Show outline: keep the contour subdued against black. */}
        {outline ? (
          <path
            d={d}
            fill="none"
            stroke={timerShown ? "rgba(255,159,10,0.65)" : "rgba(95,95,100,0.65)"}
            strokeWidth={2}
            clipPath={`url(#${clipId})`}
          />
        ) : null}
      </svg>
      <div className="island-content" style={{ clipPath: `path("${d}")` }}>
        <DrawnOffset.Provider value={offsetOf}>
          <DrawnSize.Provider value={contentSize}>{children}</DrawnSize.Provider>
        </DrawnOffset.Provider>
      </div>
    </div>
  );
}
