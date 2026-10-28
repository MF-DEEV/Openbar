// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island's countdown (NotchCapsuleCalendarStrip in
// NotchCapsuleViews.swift:582-669, NotchCalendarStrip.swift; spec 04 §3.1):
// the event's dot and title, then m:ss to its start (white) or its end
// (mint), and the time it starts or ends, which gives way first.

import { useEffect, useState } from "react";
import { edgeInset, type Geometry, shoulder } from "../island/geometry";
import type { ActivitySpec } from "../island/store";
import { useIsland } from "../island/store";
import type { ActivityViewProps } from "../island/views/Activity";
import { RollingDigits } from "../shared/ui/RollingDigits";
import { type Countdown, countdownTarget, countdownText, displayTitle, timeText } from "./model";
import { C } from "./strings";
import { useCalendar } from "./store";

const DOT = 6;
/** Beside the camera: dot to title, and clock to its time (NotchCalendarSupport.strip*Spacing). */
const TITLE_SPACING = 5;
const CLOCK_SPACING = 4;
/** In the capsule (CapsuleLayout): mark to text, title to clock, clock to time. */
const SPACING = 6;
const GROUP_SPACING = 10;
const MARK_SPACING = 4;
const CAPSULE_END = 8;
/** The capsule grows to 300 pt at most (CapsuleLayout.Maximum.calendar). */
const CAPSULE_MAX = 300;
/** Beside the camera, each wing takes 72-120 pt (calendarWingRange). */
const WING_MIN = 72;
const WING_MAX = 120;
const FONT = "'Inter Variable', Inter, sans-serif";

let canvas: HTMLCanvasElement | undefined;
function measure(text: string, font: string): number {
  if (typeof document === "undefined") return text.length * 7;
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d")!;
  ctx.font = font;
  return Math.ceil(ctx.measureText(text).width) + 2;
}

const capsule = (g: Geometry) => g.floatingGap !== null;
const titleFont = (g: Geometry) => `600 ${capsule(g) ? 12 : 11}px ${FONT}`;
const clockFont = (g: Geometry) => `${capsule(g) ? 600 : 500} 13px ${FONT}`;
const smallFont = `500 11px ${FONT}`;
const clockInset = (stripHeight: number, g: Geometry) =>
  edgeInset({ width: 300, height: stripHeight }, 9, 0, 5, g.input);

/** How wide the strip draws: the title and the widest clock the hour shows, with the time beside it. */
export function stripSpec(c: Countdown, g: Geometry): ActivitySpec {
  const title = measure(displayTitle(c.event, C.untitled), titleFont(g));
  const clock = measure("00:00", clockFont(g));
  const time = measure(timeText(c), smallFont);
  if (capsule(g)) {
    const content = DOT + SPACING + title + GROUP_SPACING + clock + MARK_SPACING + time;
    const room = CAPSULE_MAX - 2 * (CAPSULE_END + shoulder(g.stripHeight));
    return { wing: 0, capsuleContent: Math.min(content, room), opens: "calendar" };
  }
  const titleSide = DOT + TITLE_SPACING + title;
  const clockSide = clock + CLOCK_SPACING + time;
  const wing = clockInset(g.stripHeight, g) + Math.max(titleSide, clockSide);
  return { wing: Math.min(WING_MAX, Math.max(WING_MIN, Math.ceil(wing))), opens: "calendar" };
}

/** Ticks once a second, on the second, while the strip shows. */
function useSecond(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(Date.now());
      timer = setTimeout(tick, 1000 - (Date.now() % 1000) + 5);
    };
    timer = setTimeout(tick, 1000 - (Date.now() % 1000) + 5);
    return () => clearTimeout(timer);
  }, []);
  return now;
}

/** Only the minutes roll; the seconds change in place (everySecond: false). */
function Clock({ text, ongoing }: { text: string; ongoing: boolean }) {
  const colon = text.lastIndexOf(":");
  return (
    <span className={`cal-strip-clock${ongoing ? " is-ongoing" : ""}`}>
      <RollingDigits text={text.slice(0, colon)} />
      <span>{text.slice(colon)}</span>
    </span>
  );
}

export function CalendarStrip({ geometry, size }: ActivityViewProps) {
  // An event ending clears the countdown before the strip has left.
  const shown = useCalendar((s) => s.countdown ?? s.lastCountdown);
  const now = useSecond();
  if (!shown) return null;
  const { event, ongoing } = shown;
  const title = displayTitle(event, C.untitled);
  const remaining = countdownText(countdownTarget(shown), now);
  const time = timeText(shown);
  const seconds = Math.max(0, Math.ceil((countdownTarget(shown) - now) / 1000));
  const open = () => {
    useCalendar.setState({ revealing: event.id });
    useIsland.getState().open("calendar");
  };
  const dot = <span className="cal-strip-dot" style={{ background: event.color }} aria-hidden="true" />;
  const common = {
    type: "button" as const,
    onClick: open,
    title,
    "aria-label": `${C.stripLabel(ongoing, title)}, ${C.remaining(seconds)}`,
  };

  if (capsule(geometry)) {
    const padding = CAPSULE_END + shoulder(geometry.stripHeight);
    const room = size.width - 2 * padding;
    const clockGroup = measure("00:00", clockFont(geometry)) + MARK_SPACING + measure(time, smallFont);
    // The time beside the clock gives way first, before the title is all gone.
    const showsTime = room - DOT - SPACING - GROUP_SPACING - clockGroup >= 24;
    return (
      <button
        {...common}
        className="cal-strip is-capsule"
        style={{ width: size.width, height: size.height, paddingInline: padding }}
      >
        {dot}
        <span className="cal-strip-title">{title}</span>
        <span className="cal-strip-reading">
          <Clock text={remaining} ongoing={ongoing} />
          {showsTime ? <span className="cal-strip-time">{time}</span> : null}
        </span>
      </button>
    );
  }

  const wing = Math.max(0, (size.width - geometry.cameraWidth) / 2);
  const inset = edgeInset(size, 9, 0, 5, geometry.input);
  const showsTime = measure("00:00", clockFont(geometry)) + CLOCK_SPACING + measure(time, smallFont) <= wing - inset;
  return (
    <button {...common} className="cal-strip" style={{ width: size.width, height: size.height }}>
      <span className="cal-strip-wing is-leading" style={{ width: wing, paddingLeft: inset }}>
        {dot}
        <span className="cal-strip-title">{title}</span>
      </span>
      <span style={{ width: geometry.cameraWidth, flex: "none" }} />
      <span className="cal-strip-wing" style={{ width: wing, paddingRight: inset }}>
        <span className="cal-strip-reading">
          <Clock text={remaining} ongoing={ongoing} />
          {showsTime ? <span className="cal-strip-time">{time}</span> : null}
        </span>
      </span>
    </button>
  );
}
