// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Feeds the wheel over the island to the gesture tracker (NotchService.handleGesture).
// The listener is passive: lists keep scrolling on their own.

import { useEffect } from "react";
import { useMusic } from "../music/store";
import { allowsHorizontal, allowsVertical, GestureTracker, movement } from "./gestures";
import { headerMetrics } from "./geometry";
import { geometryOf, modulesOf, presentation, useIsland, visibleActivity } from "./store";
import { visibleIdleContent } from "./idle";
import { useBattery } from "./battery";

const CONTROLS = "button:not(.n-activation),input,textarea,select,a,[role=slider],[role=listbox],[contenteditable]";

/** A list that would scroll under the pointer keeps the wheel. */
function overScroller(target: Element): boolean {
  for (let el: Element | null = target; el && !el.classList.contains("island-frame"); el = el.parentElement) {
    const { overflowY, overflowX } = getComputedStyle(el);
    if (/auto|scroll/.test(overflowY) && el.scrollHeight > el.clientHeight) return true;
    if (/auto|scroll/.test(overflowX) && el.scrollWidth > el.clientWidth) return true;
  }
  return false;
}

export function gestureFromWheel(tracker: GestureTracker, event: WheelEvent): void {
  const s = useIsland.getState();
  const target = event.target instanceof Element ? event.target : null;
  const surface = target?.closest(".island-frame");
  const quiet =
    !s.settings.gestures || !s.settings.enabled || s.held || s.capturePreview !== null || s.activityPickerOpen;
  if (quiet || !surface || event.ctrlKey || event.shiftKey || event.altKey || event.metaKey) {
    tracker.reset();
    return;
  }
  const geometry = geometryOf(s);
  const idle = visibleIdleContent(
    s.settings,
    useBattery.getState().battery,
    useMusic.getState().playback?.isPlaying ?? false,
  );
  const fromTop = event.clientY;
  let inHeader = false;
  if (s.expanded) {
    const hasRow = presentation(s, geometry, undefined, idle).hasRow;
    const header = headerMetrics(geometry, hasRow);
    const [top, height] = hasRow ? [header.headerTopInset, header.headerRowHeight] : [0, header.pageTopInset];
    inHeader = fromTop >= top && fromTop <= top + height;
  } else if (s.peeking) {
    inHeader = fromTop <= geometry.stripHeight;
  }
  // The gallery steps its rows from the wheel; only its header keeps the gesture.
  if (s.expanded && s.showingSections && !inHeader) {
    tracker.reset();
    return;
  }
  const compactMusic = visibleActivity(s) === "music" || (visibleActivity(s) === null && idle === "music");
  const musicSurface =
    modulesOf(s).includes("music") &&
    useMusic.getState().playback !== null &&
    (s.expanded ? s.selected === "music" && !s.showingSections : compactMusic && !s.notice);
  const control = !!target?.closest(CONTROLS);
  const scroll = !!target && overScroller(target);
  const place = { expanded: s.expanded, inHeader, musicSurface, control, scroll };
  const { x, y, precise } = movement(event);
  const action = tracker.handle({
    x,
    y,
    timestamp: event.timeStamp / 1000,
    precise,
    allowVertical: allowsVertical(place),
    allowHorizontal: allowsHorizontal(place),
    expanded: s.expanded,
  });
  if (action === "open") s.clickClosed();
  else if (action === "close") s.collapse();
  else if (action === "nextTrack" || action === "previousTrack") {
    const forward = action === "nextTrack";
    void useMusic.getState().command(forward ? "next" : "previous");
    nudge(forward);
  }
}

/** The strip gives a little in the swipe's direction (NotchMusicStrip's skip keyframes). */
function nudge(forward: boolean): void {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const offset = forward ? -8 : 8;
  document
    .querySelector(".m-strip")
    ?.animate(
      [
        { transform: "translateX(0)" },
        { transform: `translateX(${offset}px)`, offset: 0.26 },
        { transform: "translateX(0)" },
      ],
      { duration: 340, easing: "ease-out" },
    );
}

export function useGestures(): void {
  useEffect(() => {
    const tracker = new GestureTracker();
    const onWheel = (event: WheelEvent) => gestureFromWheel(tracker, event);
    window.addEventListener("wheel", onWheel, { passive: true });
    return () => window.removeEventListener("wheel", onWheel);
  }, []);
}
