// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// NotchRollingDigits: each digit rolls to its new value (up when the number
// grows, down when it shrinks); other characters swap in place. Reduced
// motion swaps every character instantly.

import { useRef } from "react";

export function RollingDigits(props: { text: string; className?: string; label?: string }) {
  const previous = useRef(props.text);
  const direction = useRef<1 | -1>(1);
  if (previous.current !== props.text) {
    const a = parseFloat(previous.current.replace(/[^\d.]/g, ""));
    const b = parseFloat(props.text.replace(/[^\d.]/g, ""));
    if (Number.isFinite(a) && Number.isFinite(b)) direction.current = b >= a ? 1 : -1;
    previous.current = props.text;
  }
  const chars = [...props.text];
  return (
    <span className={`u-rolling${props.className ? " " + props.className : ""}`} aria-label={props.label ?? props.text}>
      {chars.map((c, i) => {
        // Keyed from the right, so "9:59" → "10:00" keeps the seconds in place.
        const key = chars.length - i;
        return /\d/.test(c) ? (
          <span key={key} className="u-rolling-cell" aria-hidden>
            <span key={c} className={`u-rolling-digit ${direction.current > 0 ? "is-up" : "is-down"}`}>
              {c}
            </span>
          </span>
        ) : (
          <span key={key} aria-hidden>
            {c}
          </span>
        );
      })}
    </span>
  );
}
