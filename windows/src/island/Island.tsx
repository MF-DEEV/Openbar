// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island page: wires the store to the surface, the pointer and keyboard
// to the store, and the window's focus to whether the island is open.

import { useLocale } from "../i18n";

import { useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { bridge, inTauri } from "../shared/bridge";
import { roomHeight } from "./room";
import { constrainIslandPosition, insetUsableEdge, islandEdge, placeIsland, usableDisplayArea } from "./placement";
import { activitySize, alongInset, outlineGeometry, restingSize } from "./geometry";
import { arrivalTime, type Size } from "./motion";
import { MODULES } from "./modules";
import { closeTopLayer } from "./pageLayer";
import {
  type ContentTransition,
  geometryOf,
  modulesOf,
  type Presentation,
  presentation,
  compactActivities,
  visibleActivity,
  showsActivityPicker,
  transitionBetween,
  useIsland,
} from "./store";
import { ContentStage } from "./surface/ContentStage";
import { IslandSurface } from "./surface/IslandSurface";
import { DrawnSize } from "./surface/DrawnSize";
import { clearAreas } from "./surface/hits";
import { NoticeView, noticeWidth, Peek, Resting } from "./views/Closed";
import { NotificationPreview, previewSize } from "../notifications/NotificationPreview";
import { useNotifications } from "../notifications/store";
import { Expanded } from "./views/Expanded";
import { QuickAccess } from "./views/QuickAccess";
import { ActivityPicker, fitPortraitSize, portraitActivitySize, portraitPickerSize } from "./views/ActivityPicker";
import { useSessions } from "../sessions/store";
import { useVoice } from "../sessions/voice";
import { presentedSnapshot } from "../sessions/presentation";
import { DropHint } from "../files/DropHint";
import { useBattery } from "./battery";
import { visibleIdleContent } from "./idle";
import { useMusic } from "../music/store";
import { CapturePresentation } from "../screenshot-preview/CapturePresentation";
import { useIslandContextMenu } from "./contextMenu";
import { mascotShows, useMascot } from "../mascot/service";
import { MirrorSource } from "./MirrorSource";
import { useGestures } from "./useGestures";

function useWindowScreen() {
  const setScreen = useIsland((s) => s.setScreen);
  useEffect(() => {
    // The native canvas spans the selected monitor, including its taskbar.
    const update = () =>
      setScreen({ width: window.innerWidth, height: window.innerHeight }, window.devicePixelRatio || 1);
    update();
    window.addEventListener("resize", update);
    const changed = bridge.onDisplayChanged(update);
    return () => {
      window.removeEventListener("resize", update);
      void changed.then((f) => f());
    };
  }, [setScreen]);
}

function usePointer(frame: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const { pointerEnter, pointerExit } = useIsland.getState();
    if (inTauri) {
      // The Rust pointer watcher knows when the pointer crosses the island's shape.
      const unlisten = bridge.onPointer((e) => (e.inside ? pointerEnter() : pointerExit()));
      const toggle = bridge.onToggle(() => useIsland.getState().toggle());
      const openHere = bridge.onOpenHere(() => {
        const s = useIsland.getState();
        if (!s.expanded) s.clickClosed();
      });
      const outside = bridge.onOutsideClick(() => {
        const s = useIsland.getState();
        if (s.expanded && !s.capturePreview && !s.pinned && !s.held) s.collapse();
        else if (s.activityPickerOpen) s.dismissActivityPicker();
      });
      return () => {
        void unlisten.then((f) => f());
        void outside.then((f) => f());
        void toggle.then((f) => f());
        void openHere.then((f) => f());
      };
    }
    // In a plain browser the frame's own events stand in.
    const el = frame.current?.querySelector<HTMLElement>(".island-frame");
    if (!el) return;
    const onDown = (e: MouseEvent) => {
      const s = useIsland.getState();
      if (s.held) return;
      if (
        s.expanded &&
        !s.capturePreview &&
        !s.pinned &&
        !(e.target as Element).closest(".island-frame, .n-quick, .u-menu, .u-menu-backdrop")
      )
        s.collapse();
      else if (s.activityPickerOpen && !(e.target as Element).closest(".island-frame")) s.dismissActivityPicker();
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
      if (e.defaultPrevented) return;
      // Escape while a reply is spoken cancels it and gives the field back.
      if (e.key === "Escape" && !e.isComposing && useVoice.getState().cancel()) {
        e.preventDefault();
        return;
      }
      if (e.key === "Escape" && !e.isComposing && closeTopLayer()) {
        e.preventDefault();
        return;
      }
      if (!s.expanded) {
        if (e.key === "Escape" && !e.isComposing && showsActivityPicker(s)) {
          e.preventDefault();
          s.dismissActivityPicker();
        }
        return;
      }
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
function useFocus(menuOpen: boolean) {
  const expanded = useIsland((s) => s.expanded || s.activityPickerOpen);
  const focusRequest = useIsland((s) => s.focusRequest);
  const previousRequest = useRef(focusRequest);
  useEffect(() => {
    const take = menuOpen || (expanded && focusRequest !== previousRequest.current);
    previousRequest.current = focusRequest;
    void bridge.setFocusable(expanded || menuOpen, take);
  }, [expanded, focusRequest, menuOpen]);
}

/** Turned off in Settings › General: nothing drawn, and nothing left to catch the pointer. */
function Disabled() {
  useLocale();
  useEffect(() => {
    useIsland.getState().collapse();
    clearAreas();
    void bridge.setCapture(false);
  }, []);
  return null;
}

const inner = (size: Size, along: number): Size => ({
  width: size.width,
  height: Math.max(0, size.height - 2 * along),
});

/** Lays landscape content out between the ears at a side panel's ends. */
function AlongInset(props: { inset: number; children: React.ReactNode }) {
  const drawn = useContext(DrawnSize);
  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        top: props.inset,
        width: drawn?.width,
        height: drawn ? Math.max(0, drawn.height - 2 * props.inset) : undefined,
      }}
    >
      <DrawnSize.Provider value={drawn ? inner(drawn, props.inset) : drawn}>{props.children}</DrawnSize.Provider>
    </div>
  );
}

export function Island() {
  useLocale();
  useWindowScreen();
  useKeyboard();
  useGestures();
  const contextMenu = useIslandContextMenu();
  useFocus(contextMenu.isOpen);
  const frame = useRef<HTMLDivElement>(null);
  usePointer(frame);

  const state = useIsland();
  const usable = usableDisplayArea(state.screen);
  const position = constrainIslandPosition(state.settings.position, state.screen, usable);
  const edge = islandEdge(position, state.screen, usable);
  const side = edge === "left" || edge === "right";
  // Content lays out as on a capsule away from the top edge (no camera strip
  // beside the header); the outline keeps the chosen silhouette, turned onto the edge.
  const attached = state.settings.silhouette === "notch";
  const geometry = useMemo(
    () =>
      geometryOf({
        ...state,
        screen: { width: usable.right - usable.left, height: usable.bottom - usable.top },
        settings: {
          ...state.settings,
          silhouette: edge === "top" ? state.settings.silhouette : "capsule",
          // A notch hangs from its edge; only a capsule floats the saved distance away.
          capsuleFit:
            edge !== "top" && attached ? { ...state.settings.capsuleFit, drop: 0 } : state.settings.capsuleFit,
        },
      }),
    [
      state.settings,
      state.screen.width,
      state.screen.height,
      state.devicePixelRatio,
      usable.left,
      usable.right,
      usable.top,
      usable.bottom,
      edge,
      attached,
    ],
  );
  const outline = useMemo(
    () => (edge === "top" ? geometry : outlineGeometry(geometry, state.settings.silhouette)),
    [geometry, edge, state.settings.silhouette],
  );
  const replyTo = useNotifications((s) => s.replyTo);
  const measured: Size | undefined = state.notice
    ? state.noticeExpanded && state.notice.notification
      ? previewSize(state.notice.notification, geometry, replyTo === state.notice.notification.id)
      : { width: noticeWidth(state.notice, geometry), height: geometry.stripHeight }
    : undefined;
  const battery = useBattery((s) => s.battery);
  const playing = useMusic((s) => s.playback?.isPlaying ?? false);
  const idleContent = visibleIdleContent(state.settings, battery, playing);
  const mascot = useMascot();
  const mascotShown = mascotShows(state, geometry, mascot);
  const vertical = side;
  let shown = presentation(state, geometry, measured, idleContent, mascotShown);
  if (vertical) {
    const workHeight = usable.bottom - usable.top;
    const workWidth = usable.right - usable.left;
    const work = { width: workWidth, height: workHeight };
    switch (shown.kind) {
      case "resting":
        shown = { ...shown, size: fitPortraitSize({ width: shown.size.height, height: shown.size.width }, work) };
        break;
      case "activity": {
        const activity = visibleActivity(state);
        if (activity) shown = { ...shown, size: portraitActivitySize(shown.size, work, activity) };
        break;
      }
      case "picker":
        shown = {
          ...shown,
          size: portraitPickerSize(
            compactActivities(state).map((id) => {
              const spec = state.activities[id]!;
              return activitySize(geometry, spec.wing, spec.capsuleContent);
            }),
            work,
            compactActivities(state),
          ),
        };
        break;
    }
  }
  // Only the closed strips stand upright on a side edge. Panels, notices and
  // previews keep their landscape layout and grow out of the edge, their ends
  // lengthened by the outline's ears so the content clears them.
  const landscape = side && !["resting", "activity", "picker"].includes(shown.kind);
  let along = 0;
  if (landscape) {
    const grown = { width: shown.size.width, height: shown.size.height + 28 };
    along = alongInset(outline, grown, edge);
    const height = Math.min(shown.size.height + 2 * along, usable.bottom - usable.top - 16);
    shown = { ...shown, size: { width: Math.min(shown.size.width, usable.right - usable.left - 72), height } };
  }
  const previousPlacement = useRef({ edge, size: shown.size, kind: shown.kind });
  const smoothPosition =
    shown.kind === "resting" &&
    previousPlacement.current.kind === "resting" &&
    previousPlacement.current.edge === edge &&
    previousPlacement.current.size.width === shown.size.width &&
    previousPlacement.current.size.height === shown.size.height;
  // Record what was committed, not what a render (or React's second dev
  // render) only proposed, or a jump to a new place would animate as a glide.
  useLayoutEffect(() => {
    previousPlacement.current = { edge, size: shown.size, kind: shown.kind };
  });
  const resting = restingSize(geometry, idleContent !== "none");
  const closedAnchorSize = vertical
    ? fitPortraitSize({ width: resting.height, height: resting.width }, geometry.screen)
    : resting;
  const placement = placeIsland(
    position,
    state.screen,
    shown.size,
    insetUsableEdge(usable, edge, geometry.floatingDrop, state.screen),
    shown.kind === "expanded",
    closedAnchorSize.height,
    closedAnchorSize.width,
  );
  const top = placement.top;
  const waiting = useSessions((s) => presentedSnapshot(s).counts.waiting);
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

  // Maximized windows start below the closed island (Settings › Behavior, Linux).
  const room = roomHeight(state.settings, geometry);
  useEffect(() => {
    void bridge.setRoom(room);
  }, [room]);

  if (!state.settings.enabled) return <Disabled />;

  let content;
  switch (shown.kind) {
    case "picker":
      content = <ActivityPicker geometry={geometry} size={shown.size} visible vertical={vertical} edge={edge} />;
      break;
    case "expanded":
      content = state.capturePreview ? (
        <CapturePresentation id={state.capturePreview} geometry={geometry} size={inner(shown.size, along)} />
      ) : (
        <Expanded geometry={geometry} size={inner(shown.size, along)} hasRow={shown.hasRow} />
      );
      break;
    case "notice":
      content = <NoticeView notice={state.notice!} geometry={geometry} size={inner(shown.size, along)} />;
      break;
    case "noticePreview":
      content = <NotificationPreview notice={state.notice!} geometry={geometry} size={inner(shown.size, along)} />;
      break;
    case "peek":
      content = <Peek geometry={geometry} size={inner(shown.size, along)} edge={edge} />;
      break;
    case "dropHint":
      content = <DropHint geometry={geometry} size={inner(shown.size, along)} />;
      break;
    case "activity":
      content = (
        <ActivityPicker geometry={geometry} size={shown.size} visible={false} vertical={vertical} edge={edge} />
      );
      break;
    default:
      content = <Resting size={shown.size} geometry={geometry} idleContent={idleContent} vertical={vertical} />;
  }

  // Hidden until hover: the closed island stays where it is (the pointer
  // still finds it) but shows nothing, unless an activity or indicator may
  // show (Settings › Behavior › Show activity and indicators).
  const hidden =
    !contextMenu.isOpen &&
    state.settings.opening === "hiddenUntilHover" &&
    !state.inside &&
    (shown.kind === "resting" ||
      ((shown.kind === "activity" || shown.kind === "notice") && !state.settings.hiddenShowsActivity));

  if (along > 0) content = <AlongInset inset={along}>{content}</AlongInset>;

  return (
    <div className={`island-root${hidden ? " is-hidden" : ""}`} ref={frame} onContextMenu={contextMenu.onContextMenu}>
      <IslandSurface
        geometry={geometry}
        outline={outline}
        target={shown.size}
        left={placement.left}
        top={top}
        edge={edge}
        position={position}
        smoothPosition={smoothPosition}
      >
        <ContentStage
          contentKey={shown.kind === "activity" || shown.kind === "picker" ? "compact:activity" : shown.key}
          transition={transition.current}
          width={shown.size.width}
          height={shown.size.height}
          edge={edge}
          position={position}
        >
          {content}
        </ContentStage>
      </IslandSurface>
      <QuickAccess
        geometry={geometry}
        size={shown.size}
        visible={shown.kind === "expanded"}
        delay={bubbleDelay}
        left={placement.left}
        top={top}
        edge={edge}
        smoothPosition={smoothPosition}
      />
      {contextMenu.menu}
      {state.settings.display === "all" ? (
        <MirrorSource geometry={geometry} outline={outline} idleContent={idleContent} />
      ) : null}
    </div>
  );
}
