// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island page: wires the store to the surface, the pointer and keyboard
// to the store, and the window's focus to whether the island is open.

import { useEffect, useMemo, useRef, useState } from "react";
import { bridge, inTauri } from "../shared/bridge";
import { arrivalTime, type Size } from "./motion";
import { MODULES } from "./modules";
import {
  type ActivityId,
  type ContentTransition,
  geometryOf,
  modulesOf,
  type Presentation,
  presentation,
  transitionBetween,
  useIsland,
} from "./store";
import { ContentStage } from "./surface/ContentStage";
import { IslandSurface } from "./surface/IslandSurface";
import { NoticeView, noticeWidth, Peek, Resting } from "./views/Closed";
import { Expanded } from "./views/Expanded";
import { QuickAccess } from "./views/QuickAccess";
import { ActivityStrip } from "./views/Activity";
import { useSessions } from "../sessions/store";

function useWindowScreen() {
  const setScreen = useIsland((s) => s.setScreen);
  useEffect(() => {
    // The island window spans its display's width; the display's height comes from the screen.
    const update = () =>
      setScreen({ width: window.innerWidth, height: window.screen.height }, window.devicePixelRatio || 1);
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [setScreen]);
}

function usePointer(frame: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const { pointerEnter, pointerExit } = useIsland.getState();
    if (inTauri) {
      // The Rust pointer watcher knows when the pointer crosses the island's shape.
      const unlisten = bridge.onPointer((e) => (e.inside ? pointerEnter() : pointerExit()));
      const toggle = bridge.onToggle(() => useIsland.getState().toggle());
      const outside = bridge.onOutsideClick(() => {
        const s = useIsland.getState();
        if (s.expanded && !s.pinned) s.collapse();
      });
      return () => {
        void unlisten.then((f) => f());
        void outside.then((f) => f());
        void toggle.then((f) => f());
      };
    }
    // In a plain browser the frame's own events stand in.
    const el = frame.current?.querySelector<HTMLElement>(".island-frame");
    if (!el) return;
    const onDown = (e: MouseEvent) => {
      const s = useIsland.getState();
      if (s.expanded && !s.pinned && !(e.target as Element).closest(".island-frame, .n-quick")) s.collapse();
    };
    el.addEventListener("mouseenter", pointerEnter);
    el.addEventListener("mouseleave", pointerExit);
    document.addEventListener("mousedown", onDown);
    return () => {
      el.removeEventListener("mouseenter", pointerEnter);
      el.removeEventListener("mouseleave", pointerExit);
      document.removeEventListener("mousedown", onDown);
    };
  }, [frame]);
}

/** Island shortcuts while it has the keyboard (spec 01-shell §2.6). */
function useKeyboard() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useIsland.getState();
      if (!s.expanded) return;
      if (e.key === "Escape" && !e.isComposing) {
        e.preventDefault();
        s.stepBack();
        return;
      }
      if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        s.toggleSections();
        return;
      }
      if (e.ctrlKey && !e.altKey && e.key === ",") {
        e.preventDefault();
        s.collapse();
        void bridge.openSettings();
        return;
      }
      if (e.ctrlKey && e.key === "Tab") {
        e.preventDefault();
        const modules = modulesOf(s);
        const i = modules.indexOf(s.selected);
        const next = modules[(i + (e.shiftKey ? -1 : 1) + modules.length) % modules.length];
        if (next) s.select(next);
        return;
      }
      if (e.ctrlKey && e.altKey && !e.shiftKey) {
        const target = MODULES.find((m) => m.shortcutKey === e.key.toLowerCase());
        if (target && modulesOf(s).includes(target.id)) {
          e.preventDefault();
          s.select(target.id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

/** The window takes the keyboard while the island is open, and gives it back when it closes. */
function useFocus() {
  const expanded = useIsland((s) => s.expanded);
  const focusRequest = useIsland((s) => s.focusRequest);
  const previousRequest = useRef(focusRequest);
  useEffect(() => {
    const take = expanded && focusRequest !== previousRequest.current;
    previousRequest.current = focusRequest;
    void bridge.setFocusable(expanded, take);
  }, [expanded, focusRequest]);
}

/** Turned off in Settings › General: nothing drawn, and nothing left to catch the pointer. */
function Disabled() {
  useEffect(() => {
    useIsland.getState().collapse();
    void bridge.setHitRects([]);
    void bridge.setCapture(false);
  }, []);
  return null;
}

export function Island() {
  useWindowScreen();
  useKeyboard();
  useFocus();
  const frame = useRef<HTMLDivElement>(null);
  usePointer(frame);

  const state = useIsland();
  const geometry = useMemo(
    () => geometryOf(state),
    [state.settings, state.screen.width, state.screen.height, state.devicePixelRatio],
  );
  const measured: Size | undefined = state.notice
    ? { width: noticeWidth(state.notice, geometry), height: geometry.stripHeight }
    : undefined;
  const shown = presentation(state, geometry, measured);
  const waiting = useSessions((s) => s.snapshot.counts.waiting);
  useEffect(() => {
    void bridge.attention.compactSessions(
      state.settings.enabled && !state.expanded && waiting > 0 && shown.key === "activity:sessions",
    );
  }, [state.settings.enabled, state.expanded, waiting, shown.key]);

  // The transition is decided when the presented content changes.
  const previous = useRef<Presentation>(shown);
  const transition = useRef<ContentTransition>("none");
  if (previous.current.key !== shown.key) {
    transition.current = transitionBetween(previous.current, shown);
    previous.current = shown;
  } else {
    previous.current = shown;
  }

  // Bubbles slide out once the island has arrived at its open size.
  const [bubbleDelay, setBubbleDelay] = useState(0);
  const closedSize = useRef<Size>(shown.size);
  useEffect(() => {
    if (shown.kind !== "expanded") closedSize.current = shown.size;
    else setBubbleDelay(arrivalTime(closedSize.current, shown.size));
  }, [shown.kind, shown.size.width, shown.size.height]);

  if (!state.settings.enabled) return <Disabled />;

  let content;
  switch (shown.kind) {
    case "expanded":
      content = <Expanded geometry={geometry} size={shown.size} hasRow={shown.hasRow} />;
      break;
    case "notice":
      content = <NoticeView notice={state.notice!} geometry={geometry} size={shown.size} />;
      break;
    case "peek":
      content = <Peek geometry={geometry} size={shown.size} />;
      break;
    case "activity":
      content = (
        <ActivityStrip id={shown.key.slice("activity:".length) as ActivityId} geometry={geometry} size={shown.size} />
      );
      break;
    default:
      content = <Resting size={shown.size} />;
  }

  // Hidden until hover: the closed island stays where it is (the pointer
  // still finds it) but shows nothing, unless an activity or indicator may
  // show (Settings › Behavior › Show activity and indicators).
  const hidden =
    state.settings.opening === "hiddenUntilHover" &&
    !state.inside &&
    (shown.kind === "resting" ||
      ((shown.kind === "activity" || shown.kind === "notice") && !state.settings.hiddenShowsActivity));

  return (
    <div className={`island-root${hidden ? " is-hidden" : ""}`} ref={frame}>
      <IslandSurface geometry={geometry} target={shown.size}>
        <ContentStage
          contentKey={shown.key}
          transition={transition.current}
          width={shown.size.width}
          height={shown.size.height}
        >
          {content}
        </ContentStage>
      </IslandSurface>
      <QuickAccess geometry={geometry} size={shown.size} visible={shown.kind === "expanded"} delay={bubbleDelay} />
    </div>
  );
}
