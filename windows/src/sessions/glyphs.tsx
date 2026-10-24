// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The filled phase symbols the Mac draws with SF Symbols
// (checkmark.circle.fill, hand.raised.fill), which outline icon sets lack.

import type { CSSProperties } from "react";

export interface GlyphProps {
  size?: number;
  strokeWidth?: number;
  style?: CSSProperties;
  className?: string;
}

/** A filled circle in the current color with a dark tick cut into it. */
export function CheckCircleFill({ size = 14, style, className }: GlyphProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <circle cx="12" cy="12" r="11" fill="currentColor" />
      <path
        d="M7 12.4l3.3 3.3L17.2 8.6"
        fill="none"
        stroke="#000"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** A raised open hand, filled. */
export function HandFill({ size = 14, style, className }: GlyphProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <g fill="currentColor">
        <rect x="6.6" y="4.2" width="2.9" height="10.5" rx="1.45" />
        <rect x="9.9" y="2" width="2.9" height="12" rx="1.45" />
        <rect x="13.2" y="2.8" width="2.9" height="11.5" rx="1.45" />
        <rect x="16.5" y="5" width="2.9" height="10" rx="1.45" />
        <path d="M6.6 11.5h12.8v3.6c0 4.2-2.9 7.4-6.6 7.4-2.3 0-3.9-1-5.3-2.9L3.6 14.6c-.7-.9-.5-2.1.4-2.7.8-.6 1.9-.4 2.6.4V11.5z" />
      </g>
    </svg>
  );
}

/** The four-pointed sparkle, filled, for sessions at work. */
export function SparkleFill({ size = 14, style, className }: GlyphProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <path
        d="M12 1.5c.6 4.9 2.2 8.4 10.5 10.5-8.3 2.1-9.9 5.6-10.5 10.5C11.4 17.6 9.8 14.1 1.5 12 9.8 9.9 11.4 6.4 12 1.5z"
        fill="currentColor"
      />
    </svg>
  );
}
