// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The floating buttons beside the open island (NotchQuickAccessView.swift;
// spec 01-shell §1.7): 44 pt black circles 12 pt off the island's sides and
// bottom, at most three a side, sliding out once the island has arrived.

import { useLocale, t } from "../../i18n";

import { Pin, PinOff, Settings, LayoutGrid, Undo2, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { bridge } from "../../shared/bridge";
import { type Geometry } from "../geometry";
import { moduleInfo } from "../modules";
import type { Size } from "../motion";
import { placeButtons, QUICK_LAYOUT, type PlacedQuickButton as Placed } from "../quickAccessLayout";
import { surfaceOffset, usableDisplayArea, type IslandEdge, type IslandPosition } from "../placement";
import { modulesOf, useIsland } from "../store";
import { setCircles, setCircleScale, windowIsCut } from "../surface/hits";

const DIAMETER = QUICK_LAYOUT.diameter;
/** Spring(0.38, 0.12) → stiffness (2π/0.38)², damping 2·0.88·(2π/0.38). */
const REVEAL = { type: "spring", stiffness: 273.4, damping: 29.1, mass: 1 } as const;

/** The circles grow in; where the window is cut to what is drawn they do not fade as well. */
function reveal(delay: number) {
  const transition = windowIsCut
    ? { scale: { ...REVEAL, delay }, opacity: { duration: 0, delay } }
    : { ...REVEAL, delay };
  return { scale: 1, opacity: 1, transition };
}

export function QuickAccess(props: { geometry: Geometry; size: Size; visible: boolean; delay: number; left?: number; top?: number; edge?: IslandEdge; position?: IslandPosition; closedSize?: Size; onDrawn?: React.RefObject<((size: Size) => void) | null>; smoothPosition?: boolean }) {
  useLocale();
  const container = useRef<HTMLDivElement>(null);
  const [drawn, setDrawn] = useState(props.size);
  useLayoutEffect(() => {
    if (!props.onDrawn) return;
    props.onDrawn.current = (size) => setDrawn((old) => old.width === size.width && old.height === size.height ? old : size);
    return () => { if (props.onDrawn) props.onDrawn.current = null; };
  }, [props.onDrawn]);
  const settings = useIsland((s) => s.settings);
  const modules = modulesOf({ settings });
  const shown = settings.quickAccess.filter((b) => b.action.kind !== "module" || modules.includes(b.action.module));
  const work = usableDisplayArea({ width: window.innerWidth, height: window.innerHeight });
  const placed = placeButtons(shown, props.size, props.geometry, props.edge, props.top === undefined ? undefined : {
    top: props.top - work.top, bottom: work.bottom - props.top - props.size.height,
  });
  const side = props.edge === "left" || props.edge === "right";
  const lastOpen = useRef<{ size: Size; placed: Placed[]; left: number; top: number } | null>(null);
  if (side && props.visible) lastOpen.current = {
    size: props.size, placed, left: props.left ?? 0, top: props.top ?? 0,
  };
  const closed = props.closedSize ?? props.size;
  const change = Math.max(Math.abs(props.size.width - closed.width), Math.abs(props.size.height - closed.height), 1);
  const moved = Math.max(Math.abs(drawn.width - closed.width), Math.abs(drawn.height - closed.height));
  const progress = Math.min(1, Math.max(0, moved / change));
  const offset = surfaceOffset(props.size, drawn, props.edge ?? "top", props.position);
  const openingPlaced = side && props.visible ? placed.map((p) => {
    const top = p.cy < 0;
    const bottom = p.cy > props.size.height;
    const cx = top || bottom ? closed.width / 2 + p.cx - props.size.width / 2
      : (props.edge === "left" ? closed.width : 0) + (props.edge === "left" ? QUICK_LAYOUT.offset : -QUICK_LAYOUT.offset);
    const cy = top ? p.cy : bottom ? closed.height + p.cy - props.size.height : closed.height / 2;
    return { ...p, cx: offset.x + cx + (p.cx - cx) * progress, cy: offset.y + cy + (p.cy - cy) * progress };
  }) : placed;
  const closing = side && !props.visible ? lastOpen.current : null;
  const closingSpan = closing ? Math.max(Math.abs(closing.size.width - props.size.width), Math.abs(closing.size.height - props.size.height), 1) : 1;
  const closingProgress = closing ? Math.min(1, Math.max(0,
    Math.max(Math.abs(drawn.width - props.size.width), Math.abs(drawn.height - props.size.height)) / closingSpan,
  )) : 0;
  const visualPlaced = closing ? closing.placed.map((p) => {
    const top = p.cy < 0;
    const bottom = p.cy > closing.size.height;
    const cx = top || bottom ? props.size.width / 2 + p.cx - closing.size.width / 2
      : (props.edge === "left" ? props.size.width + QUICK_LAYOUT.offset : -QUICK_LAYOUT.offset);
    const cy = top ? p.cy : bottom ? props.size.height + p.cy - closing.size.height : props.size.height / 2;
    return { ...p,
      cx: cx + (closing.left + p.cx - (props.left ?? 0) - cx) * closingProgress,
      cy: cy + (closing.top + p.cy - (props.top ?? 0) - cy) * closingProgress,
    };
  }) : openingPlaced;

  // The circles take the pointer while they show; the gaps between them do not.
  useEffect(() => {
    if (!props.visible) {
      setCircles([]);
      return;
    }
    let raf = 0;
    const started = performance.now();
    const follow = () => {
      const bounds = container.current?.getBoundingClientRect();
      const frameLeft = bounds?.left ?? props.left ?? (window.innerWidth - props.size.width) / 2;
      const frameTop = bounds?.top ?? props.top ?? props.geometry.floatingDrop;
      setCircles(visualPlaced.map((p) => ({
        id: p.button.id,
        reach: {
          x: frameLeft + p.cx - DIAMETER / 2,
          y: frameTop + p.cy - DIAMETER / 2,
          width: DIAMETER,
          height: DIAMETER,
          topRadius: DIAMETER / 2,
          bottomRadius: DIAMETER / 2,
        },
      })));
      if (performance.now() - started < 220) raf = requestAnimationFrame(follow);
    };
    follow();
    return () => { cancelAnimationFrame(raf); setCircles([]); };
  }, [
    props.visible,
    props.size.width,
    props.size.height,
    props.geometry,
    props.edge,
    props.left,
    props.top,
    drawn.width,
    drawn.height,
    closed.width,
    closed.height,
    settings.quickAccess,
    settings.hiddenModules,
    settings.sections,
  ]);

  return (
    <div
      ref={container}
      className={`n-quick${props.smoothPosition ? " is-position-moving" : ""}`}
      style={{ width: props.size.width, height: props.size.height, left: props.left, top: props.top ?? props.geometry.floatingDrop, transform: props.left === undefined ? undefined : "none" }}
    >
      <AnimatePresence>
        {visualPlaced.map((p) => (
          <Bubble key={p.button.id} placed={p} visible={props.visible} delay={props.delay} side={side} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function Bubble({ placed, visible, delay, side }: { placed: Placed; visible: boolean; delay: number; side: boolean }) {
  useLocale();
  const reducedMotion = useReducedMotion();
  const showingSections = useIsland((s) => s.showingSections);
  const pinned = useIsland((s) => s.pinned);
  const selected = useIsland((s) => s.selected);
  const { action } = placed.button;

  let icon: LucideIcon;
  let title: string;
  let isSelected = false;
  let run: () => void;
  switch (action.kind) {
    case "explore":
      icon = showingSections ? Undo2 : LayoutGrid;
      title = showingSections ? t("island.back") : t("island.sections");
      isSelected = showingSections;
      run = () => useIsland.getState().toggleSections();
      break;
    case "settings":
      icon = Settings;
      title = t("island.settings");
      run = () => {
        useIsland.getState().collapse();
        void bridge.openSettings();
      };
      break;
    case "pin":
      icon = pinned ? PinOff : Pin;
      title = pinned ? t("island.unpin") : t("island.pin");
      isSelected = pinned;
      run = () => useIsland.getState().setPinned(!pinned);
      break;
    case "module": {
      const module = action.module;
      icon = moduleInfo(module).icon;
      title = t(`module.${module}`);
      isSelected = !showingSections && selected === module;
      run = () => useIsland.getState().select(module);
      break;
    }
  }
  const Icon = icon;
  const label = placed.button.label || title;
  const id = placed.button.id;

  // The window is cut to what is drawn, so the circle is only cut out at the size it has now.
  useEffect(() => () => setCircleScale(id, 0), [id]);

  return (
    <motion.div
      layout={side ? undefined : "position"}
      onUpdate={(latest) => {
        const scale = Number(latest.scale);
        setCircleScale(id, Number(latest.opacity) > 0.01 && scale > 0 ? scale : 0);
      }}
      exit={{ scale: 0.4, opacity: 0, transition: { duration: 0.16 } }}
      style={{
        position: "absolute",
        left: placed.cx - DIAMETER / 2,
        top: placed.cy - DIAMETER / 2,
        width: DIAMETER,
        height: DIAMETER,
      }}
      initial={{ scale: 0.4, opacity: 0 }}
      animate={visible ? reveal(delay) : { scale: 0.4, opacity: 0, transition: { duration: 0.16, ease: "easeIn" } }}
    >
      <motion.button
        type="button"
        className={`n-bubble${isSelected ? " is-selected" : ""}`}
        aria-label={label}
        title={label}
        style={{ left: 0, top: 0, pointerEvents: visible ? "auto" : "none" }}
        initial={false}
        animate={{ opacity: 1, backgroundColor: "#000000" }}
        transition={reducedMotion ? { duration: 0 } : { type: "spring", stiffness: 385.53, damping: 29.45 }}
        // Keep the glyph fixed on the pixel grid; animate only the wash and opacity.
        whileHover={{ backgroundColor: "#171717" }}
        whileTap={{
          opacity: 0.8,
          transition: reducedMotion ? { duration: 0 } : { type: "spring", stiffness: 503.55, damping: 31.42 },
        }}
        onClick={run}
        tabIndex={visible ? 0 : -1}
      >
        <Icon size={19} strokeWidth={1.75} />
      </motion.button>
    </motion.div>
  );
}
