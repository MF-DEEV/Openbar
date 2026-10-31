// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../../i18n/index";

// Draws the island's outline frame by frame from the resize timeline and
// clips the content to it (NotchWindowHost.swift; spec 01-shell §3.1). The
// expanded content sits at its destination size; compact content follows
// the visible surface. Each frame also tells Rust which part of the
// window takes the pointer.

import { type ReactNode, useId, useLayoutEffect, useRef, useState } from "react";
import { useIsland, visibleActivity } from "../store";
import { bridge, type HitRect } from "../../shared/bridge";
import { type Geometry, hitShape, islandPath } from "../geometry";
import { MotionTimeline, type Size } from "../motion";
import { DrawnSize } from "./DrawnSize";

/** Extra hit areas (floating bubbles), in window CSS pixels, written by their owners. */
export const extraHits: { current: HitRect[] } = { current: [] };
/** Latest island hit area, so editing shortcuts can publish their new hit areas immediately. */
export const surfaceHits: { current: HitRect[] } = { current: [] };

const now = () => performance.now() / 1000;

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** Path `d` with its origin moved right by `dx` (path() has no transform of its own). */
function shifted(geometry: Geometry, size: Size, dx: number): string {
  if (dx === 0) return islandPath(geometry, size);
  return islandPath(geometry, size).replace(
    /(-?\d*\.?\d+(?:e-?\d+)?) (-?\d*\.?\d+(?:e-?\d+)?)/g,
    (_, x, y) => `${Number(x) + dx} ${y}`,
  );
}

export function IslandSurface(props: {
  geometry: Geometry;
  target: Size;
  onDrawn?: (size: Size, settled: boolean) => void;
  children: ReactNode;
}) {
  useLocale();
  const { geometry, target, children } = props;
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
  const lastHits = useRef("");
  const onDrawn = useRef(props.onDrawn);
  onDrawn.current = props.onDrawn;

  useLayoutEffect(() => {
    const draw = (size: Size, settled: boolean) => {
      drawn.current = size;
      setContentSize((previous) =>
        previous.width === size.width && previous.height === size.height ? previous : size,
      );
      reportHits(size);
      onDrawn.current?.(size, settled);
    };

    const reportHits = (size: Size) => {
      const frame = frameRef.current;
      if (!frame) return;
      const left = frame.getBoundingClientRect().left;
      // Hover and clicks answer to the larger of what is drawn and where it is going.
      const reach = { width: Math.max(size.width, target.width), height: Math.max(size.height, target.height) };
      const hit = hitShape(geometry, reach);
      const rects: HitRect[] = [
        {
          x: left + (target.width - reach.width) / 2 + hit.rect.x,
          y: geometry.floatingDrop + hit.rect.y,
          width: hit.rect.width,
          height: hit.rect.height,
          topRadius: hit.topRadius,
          bottomRadius: hit.bottomRadius,
        },
        ...extraHits.current,
      ];
      const key = JSON.stringify(rects);
      surfaceHits.current = rects.slice(0, 1);
      if (key !== lastHits.current) {
        lastHits.current = key;
        void bridge.setHitRects(rects);
      }
    };

    const t = now();
    const current = timeline.current;
    // Sample the running spring at the retarget time instead of restarting
    // from the previous animation frame when the pointer changes direction.
    const from = current ? current.sizeAt(t) : drawn.current;
    if (reducedMotion() || (from.width === target.width && from.height === target.height && !current)) {
      timeline.current = null;
      draw(target, true);
      return;
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
  }, [geometry, target.width, target.height]);

  // Commit the outline, clip and content layout together. Imperatively
  // moving the outline ahead of React's content update causes visible jumps.
  const d = shifted(geometry, contentSize, (target.width - contentSize.width) / 2);

  return (
    <div
      ref={frameRef}
      className="island-frame"
      style={{ width: target.width, height: target.height, top: geometry.floatingDrop }}
    >
      <svg className="island-backdrop" width={target.width} height={target.height} aria-hidden>
        <clipPath id={clipId}>
          <use href={`#${clipId}-shape`} />
        </clipPath>
        <path id={`${clipId}-shape`} d={d} fill="#000" />
        {/* Settings › Layout › Show outline: a 2 pt stroke of which only the
            inner 1 pt shows, white at 65% (spec 01-shell §1.8). */}
        {outline ? (
          <path
            d={d}
            fill="none"
            stroke={timerShown ? "rgba(255,159,10,0.65)" : "rgba(255,255,255,0.65)"}
            strokeWidth={2}
            clipPath={`url(#${clipId})`}
          />
        ) : null}
      </svg>
      <div className="island-content" style={{ clipPath: `path("${d}")` }}>
        <DrawnSize.Provider value={contentSize}>{children}</DrawnSize.Provider>
      </div>
    </div>
  );
}
