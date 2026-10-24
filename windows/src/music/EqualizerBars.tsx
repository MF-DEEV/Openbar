// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The equalizer's synthetic motion (NotchEqualizerBars.swift; spec 04 §1.10):
// each bar swings between a low and a high height on its own period, louder
// in the middle, eased by (1 − cos πt)/2, stepped at ≤30 fps. Paused, the
// bars rest as dots; reduced motion keeps them still.

import { useEffect, useRef } from "react";

function reduced() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

export function barHeights(n: number, width: number, height: number, t: number, moving: boolean): number[] {
  const c = (n - 1) / 2;
  return Array.from({ length: n }, (_, i) => {
    if (!moving) return width;
    const env = Math.pow(1 - Math.abs(i - c) / Math.max(1, c), 1.5);
    const low = Math.max(width, height * (0.12 + 0.25 * env));
    const high = Math.max(width, height * (0.12 + 0.88 * env));
    const half = Math.PI / (5.2 + 0.61 * i);
    const phase = (t + 0.17 * i) / half;
    const k = phase % 2 < 1 ? phase % 1 : 1 - (phase % 1);
    const eased = (1 - Math.cos(Math.PI * k)) / 2;
    return low + (high - low) * eased;
  });
}

export function EqualizerBars(props: { count: number; width: number; height: number; color: string; moving: boolean }) {
  const { count, width, height, color, moving } = props;
  const refs = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    const apply = (t: number, move: boolean) => {
      barHeights(count, width, height, t, move).forEach((h, i) => {
        const el = refs.current[i];
        if (el) el.style.height = `${h}px`;
      });
    };
    if (!moving || reduced()) {
      apply(0, false);
      return;
    }
    let raf = 0;
    let last = 0;
    const step = (ms: number) => {
      if (ms - last >= 33 && !document.hidden) {
        last = ms;
        apply(ms / 1000, true);
      }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [count, width, height, moving]);

  return (
    <span className="m-bars" style={{ height, gap: width * 0.85 }} aria-hidden>
      {Array.from({ length: count }, (_, i) => (
        <span
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          style={{ width, height: width, borderRadius: width / 2, background: color }}
        />
      ))}
    </span>
  );
}
