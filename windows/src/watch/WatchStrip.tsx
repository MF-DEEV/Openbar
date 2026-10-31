// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// The closed island while an area is watched (NotchWatchStrip beside the
// camera, NotchCapsuleWatchStrip in the capsule; spec 04 §6.2): the eye,
// breathing while the area is read and crossed out while its window is
// hidden, and what the area reads in purple, or the area itself, 40 pt
// wide, when it holds no text. Clicking opens Watch.

import { Eye, EyeOff } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { useEffect, useRef } from "react";
import { edgeInset } from "../island/geometry";
import { useIsland } from "../island/store";
import type { ActivityViewProps } from "../island/views/Activity";
import { textWidth } from "../timer/TimerStrip";
import { showsThumbnail, useWatch } from "./store";
import { S } from "./strings";
import type { WatchSnapshot } from "./types";

/** systemPurple; the eye softer than the reading so it doesn't outshine it. */
export const PURPLE = "#bf5af2";
const EYE_TINT = "rgba(191, 90, 242, 0.75)";

/** Wings 56…150 pt (NotchWatchSupport.stripWingRange); an area without text shows 40 pt of itself. */
const WING = { min: 56, max: 150, cameraGap: 6 };
export const THUMBNAIL_WIDTH = 40;
/** In the capsule: a 12 pt eye in a 16 pt slot, 6 pt from a 12 pt reading, at most 220 pt wide (NotchCapsuleLayout). */
const CAPSULE = { symbol: 12, slot: 16, spacing: 6, reading: 12, spinner: 16, maximum: 220, endPadding: 8 };

/** min(16, h − 6), as the timer's reading. */
export const stripTextSize = (stripHeight: number) => Math.min(16, stripHeight - 6);
/** A working agent's mark, as the agents strip sizes it: min(14, max(8, h − 6)). */
export const stripEyeSize = (stripHeight: number) => Math.min(14, Math.max(8, stripHeight - 6));

const readingFont = (size: number) => `500 ${size}px 'Inter Variable', Inter, sans-serif`;

/** The wider side: the reading or the area's picture, clear of the curve, with air beside the camera (NotchService.watchStripWing). */
export function stripWing(s: WatchSnapshot, stripHeight: number): number {
  const size = stripTextSize(stripHeight);
  const reading = showsThumbnail(s) ? THUMBNAIL_WIDTH : Math.ceil(textWidth(s.headline, readingFont(size)));
  const inset = edgeInset({ width: 300, height: stripHeight }, size * 0.72, 0);
  return Math.min(WING.max, Math.max(WING.min, Math.ceil(reading + inset + WING.cameraGap)));
}

/** The capsule's one row: the eye, then the reading, the picture or a spinner (NotchCapsuleLayout.watchSurface). */
export function capsuleContent(s: WatchSnapshot): number {
  const right = showsThumbnail(s)
    ? THUMBNAIL_WIDTH
    : s.headline === ""
      ? CAPSULE.spinner
      : Math.ceil(textWidth(s.headline, readingFont(CAPSULE.reading)));
  return Math.min(CAPSULE.maximum - 2 * CAPSULE.endPadding, CAPSULE.slot + CAPSULE.spacing + right);
}

/**
 * Where a swing to and fro stands at `phase`, counted in half cycles: 0 at
 * its start, 1 at its far end and 0 again two halves later, easing in and
 * out at both ends (NotchDecorativeClock.swing).
 */
export function swing(phase: number): number {
  const half = phase % 2;
  const travelled = half < 0 ? half + 2 : half;
  const out = travelled <= 1 ? travelled : 2 - travelled;
  return (1 - Math.cos(Math.PI * out)) / 2;
}

/** Scale 0.84 → 1 and opacity 0.7 → 1 over a 1.2 s half cycle (NotchAgentAnimationView.show). */
export function breath(seconds: number): { scale: number; opacity: number } {
  const b = swing(seconds / 1.2);
  return { scale: 0.84 + 0.16 * b, opacity: 0.7 + 0.3 * b };
}

/** The watching eye: breathing at ≤ 30 fps while visible, still under reduced motion, crossed out while hidden. */
export function WatchEye({ size, hidden }: { size: number; hidden: boolean }) {
  useLocale();
  const ref = useRef<HTMLSpanElement>(null);
  const reduced = useReducedMotion() ?? false;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rest = () => {
      el.style.transform = "";
      el.style.opacity = "";
    };
    if (hidden || reduced) return rest();
    const start = performance.now();
    let frame = 0;
    let last = -Infinity;
    const step = (ms: number) => {
      if (ms - last >= 33 && !document.hidden) {
        last = ms;
        const { scale, opacity } = breath((ms - start) / 1000);
        el.style.transform = `scale(${scale})`;
        el.style.opacity = String(opacity);
      }
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => {
      cancelAnimationFrame(frame);
      rest();
    };
  }, [hidden, reduced]);
  const Icon = hidden ? EyeOff : Eye;
  return (
    <span ref={ref} className="wt-eye" style={{ width: size + 2, height: size + 2, color: EYE_TINT }} aria-hidden>
      <Icon size={size} strokeWidth={2.25} />
    </span>
  );
}

function Thumbnail({ src, height }: { src: string; height: number }) {
  useLocale();
  return <img className="wt-thumbnail" src={src} alt="" style={{ width: THUMBNAIL_WIDTH, height }} />;
}

export function WatchStrip({ geometry, size }: ActivityViewProps) {
  useLocale();
  const s = useWatch((state) => state.snapshot);
  const hidden = s.phase === "hidden";
  const label = s.headline ? `${S.title}: ${s.headline}` : S.title;
  const open = () => useIsland.getState().open("watch");

  if (geometry.floatingGap !== null) {
    const body = size.height - 2 * geometry.floatingGap;
    return (
      <button
        type="button"
        className="wt-strip is-capsule"
        style={{ width: size.width, height: size.height, gap: CAPSULE.spacing }}
        aria-label={label}
        title={label}
        onClick={open}
      >
        <span className="wt-strip-slot" style={{ width: CAPSULE.slot }}>
          <WatchEye size={CAPSULE.symbol} hidden={hidden} />
        </span>
        {showsThumbnail(s) ? (
          <Thumbnail src={s.preview!} height={Math.max(8, body - 6)} />
        ) : s.headline === "" ? (
          // A hidden window's slashed eye says enough until it is read.
          <span className="wt-strip-slot" style={{ width: CAPSULE.spinner }}>
            {hidden ? null : <span className="wt-spinner" />}
          </span>
        ) : (
          <span className="wt-strip-reading" style={{ fontSize: CAPSULE.reading }}>
            {s.headline}
          </span>
        )}
      </button>
    );
  }

  const wing = (size.width - geometry.cameraWidth) / 2;
  const eye = stripEyeSize(geometry.stripHeight);
  const textSize = stripTextSize(geometry.stripHeight);
  return (
    <button
      type="button"
      className="wt-strip"
      style={{ width: size.width, height: size.height }}
      aria-label={label}
      title={label}
      onClick={open}
    >
      <span
        className="wt-strip-wing"
        style={{ width: wing, paddingLeft: edgeInset(size, eye, eye / 2, 5, geometry.input) }}
      >
        {wing >= 28 ? <WatchEye size={eye} hidden={hidden} /> : null}
      </span>
      <span style={{ width: geometry.cameraWidth, flex: "none" }} />
      <span
        className="wt-strip-wing is-end"
        style={{ width: wing, paddingRight: edgeInset(size, textSize * 0.72, 0, 5, geometry.input) }}
      >
        {wing < 36 ? null : showsThumbnail(s) ? (
          <Thumbnail src={s.preview!} height={Math.max(8, geometry.stripHeight - 6)} />
        ) : s.headline === "" ? (
          hidden ? null : (
            <span className="wt-spinner" />
          )
        ) : (
          // A reading can change on every pass; rolling each change would keep the island animating.
          <span className="wt-strip-reading" style={{ fontSize: textSize, color: PURPLE }}>
            {s.headline}
          </span>
        )}
      </span>
    </button>
  );
}
