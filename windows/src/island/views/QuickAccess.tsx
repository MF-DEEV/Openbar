// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../../i18n";

// The floating buttons beside the open island (NotchQuickAccessView.swift;
// spec 01-shell §1.7): 44 pt black circles 12 pt off the island's sides and
// bottom, at most three a side, sliding out once the island has arrived.

import { Pin, PinOff, Settings, LayoutGrid, Undo2, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect } from "react";

import { bridge } from "../../shared/bridge";
import { type Geometry } from "../geometry";
import { moduleInfo } from "../modules";
import type { Size } from "../motion";
import { placeButtons, QUICK_LAYOUT, type PlacedQuickButton as Placed } from "../quickAccessLayout";
import { modulesOf, useIsland } from "../store";
import { extraHits, surfaceHits } from "../surface/IslandSurface";

const DIAMETER = QUICK_LAYOUT.diameter;
/** Spring(0.38, 0.12) → stiffness (2π/0.38)², damping 2·0.88·(2π/0.38). */
const REVEAL = { type: "spring", stiffness: 273.4, damping: 29.1, mass: 1 } as const;

export function QuickAccess(props: { geometry: Geometry; size: Size; visible: boolean; delay: number }) {
  useLocale();
  const settings = useIsland((s) => s.settings);
  const modules = modulesOf({ settings });
  const shown = settings.quickAccess.filter((b) => b.action.kind !== "module" || modules.includes(b.action.module));
  const placed = placeButtons(shown, props.size, props.geometry);

  // The circles take the pointer while they show; the gaps between them do not.
  useEffect(() => {
    if (!props.visible) {
      extraHits.current = [];
      void bridge.setHitRects(surfaceHits.current);
      return;
    }
    const frameLeft = (window.innerWidth - props.size.width) / 2;
    extraHits.current = placed.map((p) => ({
      x: frameLeft + p.cx - DIAMETER / 2,
      y: props.geometry.floatingDrop + p.cy - DIAMETER / 2,
      width: DIAMETER,
      height: DIAMETER,
      topRadius: DIAMETER / 2,
      bottomRadius: DIAMETER / 2,
    }));
    void bridge.setHitRects([...surfaceHits.current, ...extraHits.current]);
    return () => {
      extraHits.current = [];
    };
  }, [
    props.visible,
    props.size.width,
    props.size.height,
    props.geometry,
    settings.quickAccess,
    settings.hiddenModules,
    settings.sections,
  ]);

  return (
    <div
      className="n-quick"
      style={{ width: props.size.width, height: props.size.height, top: props.geometry.floatingDrop }}
    >
      <AnimatePresence>
        {placed.map((p) => (
          <Bubble key={p.button.id} placed={p} visible={props.visible} delay={props.delay} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function Bubble({ placed, visible, delay }: { placed: Placed; visible: boolean; delay: number }) {
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

  return (
    <motion.div
      layout="position"
      exit={{ scale: 0.4, opacity: 0, transition: { duration: 0.16 } }}
      style={{
        position: "absolute",
        left: placed.cx - DIAMETER / 2,
        top: placed.cy - DIAMETER / 2,
        width: DIAMETER,
        height: DIAMETER,
      }}
      initial={{ scale: 0.4, opacity: 0 }}
      animate={
        visible
          ? { scale: 1, opacity: 1, transition: { ...REVEAL, delay } }
          : { scale: 0.4, opacity: 0, transition: { duration: 0.16, ease: "easeIn" } }
      }
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
