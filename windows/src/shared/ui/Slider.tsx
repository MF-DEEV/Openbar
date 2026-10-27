// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// NotchLevelSlider (horizontal, 24 tall with an 8 pt track) and the mixer's
// vertical fader. Values are 0…max. While the person drags, outside updates
// are ignored; otherwise they ease in (spec 05 §1.1).

import { type CSSProperties, useRef, useState } from "react";

export interface SliderProps {
  value: number;
  onChange(value: number): void;
  /** Called once when a drag or key press ends. */
  onCommit?(value: number): void;
  max?: number;
  /** A marker on the track (the mixer's 100%). */
  tick?: number;
  disabled?: boolean;
  label: string;
  /** Fill colour; white by default. */
  tint?: string;
  vertical?: boolean;
  /** Track thickness: 8 for levels, 28 for faders. */
  thickness?: number;
  /** Extra text for screen readers, e.g. "40%". */
  valueText?: string;
  className?: string;
  style?: CSSProperties;
}

export function Slider(props: SliderProps) {
  const max = props.max ?? 1;
  const [dragging, setDragging] = useState(false);
  const track = useRef<HTMLDivElement>(null);
  const last = useRef(props.value);
  const clamped = Math.min(max, Math.max(0, props.value));
  const fraction = max > 0 ? clamped / max : 0;

  const valueAt = (e: { clientX: number; clientY: number }) => {
    const r = track.current!.getBoundingClientRect();
    const f = props.vertical ? (r.bottom - e.clientY) / r.height : (e.clientX - r.left) / r.width;
    return Math.min(1, Math.max(0, f)) * max;
  };
  const set = (v: number) => {
    last.current = v;
    props.onChange(v);
  };

  return (
    <div
      ref={track}
      role="slider"
      tabIndex={props.disabled ? -1 : 0}
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={Math.round(max * 100)}
      aria-valuenow={Math.round(clamped * 100)}
      aria-valuetext={props.valueText}
      aria-orientation={props.vertical ? "vertical" : "horizontal"}
      aria-disabled={props.disabled}
      className={`u-slider${props.vertical ? " is-vertical" : ""}${dragging ? " is-dragging" : ""}${props.className ? " " + props.className : ""}`}
      style={
        {
          ...props.style,
          "--u-fill": `${fraction * 100}%`,
          "--u-tint": props.tint ?? "#fff",
          "--u-thickness": `${props.thickness ?? 8}px`,
        } as CSSProperties
      }
      onPointerDown={(e) => {
        if (props.disabled || e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        setDragging(true);
        set(valueAt(e));
      }}
      onPointerMove={(e) => {
        if (dragging) set(valueAt(e));
      }}
      onPointerUp={(e) => {
        if (!dragging) return;
        setDragging(false);
        e.currentTarget.releasePointerCapture(e.pointerId);
        props.onCommit?.(last.current);
      }}
      onPointerCancel={() => setDragging(false)}
      onWheel={(e) => {
        if (props.disabled) return;
        const delta = Math.abs(e.deltaY) > Math.abs(e.deltaX) ? -e.deltaY : e.deltaX;
        if (delta === 0) return;
        const v = Math.min(max, Math.max(0, clamped + Math.sign(delta) * 0.02 * max));
        set(v);
        props.onCommit?.(v);
      }}
      onKeyDown={(e) => {
        if (props.disabled) return;
        const step = (e.shiftKey ? 0.1 : 0.02) * max;
        const keys: Record<string, number> = {
          ArrowRight: step,
          ArrowUp: step,
          ArrowLeft: -step,
          ArrowDown: -step,
          PageUp: 0.1 * max,
          PageDown: -0.1 * max,
        };
        let v: number | undefined;
        if (e.key in keys) v = clamped + keys[e.key]!;
        else if (e.key === "Home") v = 0;
        else if (e.key === "End") v = max;
        if (v === undefined) return;
        e.preventDefault();
        e.stopPropagation();
        v = Math.min(max, Math.max(0, v));
        set(v);
        props.onCommit?.(v);
      }}
    >
      <div className="u-slider-track">
        <div className="u-slider-fill" />
        {props.tick !== undefined && max > 0 ? (
          <div className="u-slider-tick" style={{ "--u-tick": `${(props.tick / max) * 100}%` } as CSSProperties} />
        ) : null}
      </div>
    </div>
  );
}
