// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// The closed island while a timer runs (NotchTimerStrip.swift beside the
// camera, NotchCapsuleTimerStrip in the capsule; spec 04 §4.3): the timer's
// orange mark and its short reading, whose minutes roll while the seconds
// change in place. Either side opens the Timer page.

import { CircleCheck, CirclePause, Hourglass, type LucideIcon, Timer } from "lucide-react";
import { edgeInset, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { RollingDigits } from "../shared/ui/RollingDigits";
import * as model from "./model";
import { sessionTitle, useTimer } from "./store";

/** A countdown's glyph, or the stopwatch's (NotchTimerSupport.ModePicker.symbol). */
export const modeGlyph = (mode: model.TimerMode): LucideIcon => (mode === "stopwatch" ? Timer : Hourglass);

/** The mark: finished, paused, or the mode's glyph. */
export function markIcon(s: model.TimerSession): LucideIcon {
  if (s.completed) return CircleCheck;
  return model.isPaused(s) ? CirclePause : modeGlyph(s.mode);
}

/** Beside the camera: text min(16, h − 6) pt medium, the mark min(20, h − 10) (NotchTimerSupport.strip*Size). */
export const stripTextSize = (stripHeight: number) => Math.min(16, stripHeight - 6);
export const stripIconSize = (stripHeight: number) => Math.min(20, stripHeight - 10);
/** Wings 44…64, and the air between the camera and what sits beside it. */
const WING = { min: 44, max: 64, cameraGap: 6 };
/** In the capsule: a 12 pt mark in a 16 pt slot, 6 pt from a 13 pt semibold reading, at most 220 pt wide. */
const CAPSULE = { symbol: 12, slot: 16, spacing: 6, reading: 13, air: 2, maximum: 220, endPadding: 8 };

const stripFont = (size: number) => `500 ${size}px 'Inter Variable', Inter, sans-serif`;
const capsuleFont = `600 ${CAPSULE.reading}px 'Inter Variable', Inter, sans-serif`;

/** Each wing is as wide as the wider side needs (NotchService.timerStripWing). */
export function stripWing(reading: string, stripHeight: number): number {
  const box = { width: 300, height: stripHeight };
  const size = stripTextSize(stripHeight);
  const text = Math.ceil(textWidth(model.readingShape(reading), stripFont(size))) + edgeInset(box, size * 0.72, 0);
  const icon = stripIconSize(stripHeight);
  const mark = icon + edgeInset(box, icon, icon / 2);
  return Math.min(WING.max, Math.max(WING.min, Math.ceil(Math.max(text, mark) + WING.cameraGap)));
}

/** The capsule's one-row content, measured by the reading's shape so it only moves when a character comes or goes. */
export function capsuleContent(reading: string): number {
  const text = Math.ceil(textWidth(model.readingShape(reading), capsuleFont)) + CAPSULE.air;
  return Math.min(CAPSULE.maximum - 2 * CAPSULE.endPadding, CAPSULE.slot + CAPSULE.spacing + text);
}

export function TimerStrip({ geometry, size }: { geometry: Geometry; size: Size }) {
  useLocale();
  const session = useTimer((s) => s.session);
  const now = useTimer((s) => s.now);
  const reading = model.compactText(session, now);
  const Mark = markIcon(session);
  const label = `${sessionTitle(session)}: ${model.clockTextFor(session, now)}`;
  const open = () => useIsland.getState().open("timer");

  if (geometry.floatingGap !== null) {
    return (
      <button
        type="button"
        className="tm-strip is-capsule"
        style={{ width: size.width, height: size.height, gap: CAPSULE.spacing }}
        aria-label={label}
        title={label}
        onClick={open}
      >
        <span className="tm-strip-mark" style={{ width: CAPSULE.slot }}>
          <Mark size={CAPSULE.symbol + 2} strokeWidth={2.25} aria-hidden />
        </span>
        <Reading text={reading} size={CAPSULE.reading} weight={600} />
      </button>
    );
  }

  const wing = (size.width - geometry.cameraWidth) / 2;
  const iconSize = stripIconSize(geometry.stripHeight);
  const textSize = stripTextSize(geometry.stripHeight);
  return (
    // The whole strip opens the timer, the camera gap included, like the other strips.
    <div className="tm-strip" style={{ width: size.width, height: size.height }} onClick={open}>
      <button
        type="button"
        className="tm-strip-wing"
        style={{ width: wing, paddingLeft: edgeInset(size, iconSize, iconSize / 2, 5, geometry.input) }}
        aria-label={label}
        title={label}
      >
        {wing >= 28 ? <Mark size={iconSize} strokeWidth={2} aria-hidden /> : null}
      </button>
      <span style={{ width: geometry.cameraWidth, flex: "none" }} />
      <button
        type="button"
        className="tm-strip-wing is-end"
        style={{ width: wing, paddingRight: edgeInset(size, textSize * 0.72, 0, 5, geometry.input) }}
        aria-label={label}
        title={label}
      >
        {wing >= 42 ? <Reading text={reading} size={textSize} weight={500} /> : null}
      </button>
    </div>
  );
}

/** Only the part before the last colon rolls; the seconds change in place to save energy. */
function Reading({ text, size, weight }: { text: string; size: number; weight: number }) {
  useLocale();
  const [head, tail] = model.rollingSplit(text);
  return (
    <span className="tm-strip-reading" style={{ fontSize: size, fontWeight: weight }} aria-hidden>
      <RollingDigits text={head} />
      {tail}
    </span>
  );
}

let canvas: HTMLCanvasElement | undefined;
/** A line's width in `font`, for layout before anything is drawn. */
export function textWidth(text: string, font: string): number {
  if (typeof document === "undefined") return text.length * 8;
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return text.length * 8;
  ctx.font = font;
  return ctx.measureText(text).width;
}
