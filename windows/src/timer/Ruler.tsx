// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// The minutes ruler (NotchTimerRuler.swift; spec 04 §4.2): a drawn slider
// over 1…180 minutes, 14 pt a minute, the selected minute under a centre
// pointer. Drag it (left raises the value), click a tick to bring it to the
// centre, scroll it, or use the arrow keys, Home and End.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { durationText, rulerLabel, rulerMinute, rulerMoving, rulerOffset, RULER_SPACING } from "./model";

/** The height the ruler is drawn for; a shorter one compresses the ticks and pointer under the labels. */
export const RULER_HEIGHT = 82;
/** A pointer that travels less than this is a click. */
const CLICK_SLOP = 3;
/** Scroll that stops for this long starts a new gesture. */
const SCROLL_IDLE_MS = 200;

const ORANGE_RGB = "255, 159, 10";

export function Ruler(props: { value: number; onChange(minutes: number): void; label: string; height: number }) {
  useLocale();
  const { value, onChange, height } = props;
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const drag = useRef<{ x: number; value: number; moved: boolean; id: number } | null>(null);
  const scroll = useRef<{ value: number; at: number } | null>(null);
  const contrast = useHighContrast();
  const selected = rulerMinute(value);

  useLayoutEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setWidth(el.clientWidth));
    observer.observe(el);
    setWidth(el.clientWidth);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    el.width = Math.round(width * dpr);
    el.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    draw(ctx, width, height, selected, contrast);
  }, [width, height, selected, contrast]);

  const select = (next: number) => {
    const minute = rulerMinute(next);
    if (minute !== selected) onChange(minute);
  };

  return (
    <canvas
      ref={canvas}
      className="tm-ruler"
      style={{ height }}
      role="slider"
      tabIndex={0}
      aria-label={props.label}
      aria-valuemin={1}
      aria-valuemax={180}
      aria-valuenow={selected}
      aria-valuetext={durationText(selected * 60)}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, value, moved: false, id: e.pointerId };
        scroll.current = null;
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || d.id !== e.pointerId) return;
        if (!d.moved && Math.abs(e.clientX - d.x) < CLICK_SLOP) return;
        const next = rulerMoving(d.value, e.clientX - d.x);
        drag.current = { ...d, x: e.clientX, value: next, moved: true };
        select(next);
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        if (!d || d.moved || d.id !== e.pointerId) return;
        // A click brings the tick under the pointer to the centre.
        const rect = e.currentTarget.getBoundingClientRect();
        select(rulerMoving(value, -(e.clientX - rect.left - rect.width / 2)));
      }}
      onPointerCancel={() => (drag.current = null)}
      onWheel={(e) => {
        if (drag.current) return;
        const horizontal = Math.abs(e.deltaX) > Math.abs(e.deltaY);
        const delta = horizontal ? e.deltaX : e.deltaY;
        // A mouse wheel's notch (120 legacy units) moves a minute; a touchpad's pixels move as far as dragged.
        const legacy = (e.nativeEvent as WheelEvent & { wheelDeltaX?: number; wheelDeltaY?: number })[
          horizontal ? "wheelDeltaX" : "wheelDeltaY"
        ];
        const notches = legacy && legacy % 120 === 0 ? -legacy / 120 : null;
        const points =
          notches !== null
            ? notches * RULER_SPACING
            : delta * (e.deltaMode === WheelEvent.DOM_DELTA_LINE ? RULER_SPACING : 1);
        const now = performance.now();
        const from = scroll.current && now - scroll.current.at < SCROLL_IDLE_MS ? scroll.current.value : value;
        // The DOM's deltas run opposite to the Mac's scrolling deltas.
        const next = rulerMoving(from, -points);
        scroll.current = notches !== null ? null : { value: next, at: now };
        select(next);
      }}
      onKeyDown={(e) => {
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        const step: Record<string, number> = {
          ArrowLeft: selected - 1,
          ArrowDown: selected - 1,
          ArrowRight: selected + 1,
          ArrowUp: selected + 1,
          Home: 1,
          End: 180,
        };
        const next = step[e.key];
        if (next === undefined) return;
        e.preventDefault();
        scroll.current = null;
        select(next);
      }}
    />
  );
}

function draw(ctx: CanvasRenderingContext2D, width: number, height: number, selected: number, contrast: boolean) {
  ctx.clearRect(0, 0, width, height);
  const mid = width / 2;
  const edgeFade = Math.max(1, width * 0.12);
  const scale = height / RULER_HEIGHT;
  ctx.font = "500 14px 'Inter Variable', Inter, sans-serif";
  ctx.textBaseline = "top";
  ctx.textAlign = "center";
  for (let minute = 1; minute <= 180; minute++) {
    const x = mid + rulerOffset(minute, selected);
    if (x < -16 || x > width + 16) continue;
    const fade = Math.min(1, Math.max(0, Math.min(x, width - x) / edgeFade));
    const strength = minute <= selected ? 1 : contrast ? 0.65 : 0.35;
    ctx.fillStyle = `rgba(${ORANGE_RGB}, ${fade * strength})`;
    ctx.beginPath();
    ctx.roundRect(x - 2, 26 * scale, 4, 38 * scale, 2);
    ctx.fill();
    if (minute === 1 || minute % 5 === 0) ctx.fillText(rulerLabel(minute), x, 0);
  }
  ctx.fillStyle = `rgb(${ORANGE_RGB})`;
  ctx.beginPath();
  ctx.moveTo(mid, 70 * scale);
  ctx.lineTo(mid + 6, 80 * scale);
  ctx.lineTo(mid - 6, 80 * scale);
  ctx.closePath();
  ctx.fill();
}

/** Increased contrast lifts the unselected ticks (0.35 → 0.65). */
function useHighContrast(): boolean {
  const query = "(prefers-contrast: more), (forced-colors: active)";
  const [on, setOn] = useState(() => window.matchMedia?.(query).matches ?? false);
  useEffect(() => {
    const list = window.matchMedia?.(query);
    if (!list) return;
    const change = () => setOn(list.matches);
    list.addEventListener("change", change);
    return () => list.removeEventListener("change", change);
  }, []);
  return on;
}
