// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The floating buttons beside the open island (NotchQuickAccessView.swift;
// spec 01-shell §1.7): 44 pt black circles 12 pt off the island's sides and
// bottom, at most three a side, sliding out once the island has arrived.

import { Pin, PinOff, Settings, LayoutGrid, Undo2, type LucideIcon } from "lucide-react";
import { motion } from "motion/react";
import { useEffect } from "react";
import { t } from "../../i18n";
import { bridge } from "../../shared/bridge";
import { type Geometry, shoulder } from "../geometry";
import { moduleInfo } from "../modules";
import type { Size } from "../motion";
import type { QuickButton } from "../settings";
import { modulesOf, useIsland } from "../store";
import { extraHits } from "../surface/IslandSurface";

const DIAMETER = 44;
const OFFSET = 34;
const ROW = 54;
/** Spring(0.38, 0.12) → stiffness (2π/0.38)², damping 2·0.88·(2π/0.38). */
const REVEAL = { type: "spring", stiffness: 273.4, damping: 29.1, mass: 1 } as const;

interface Placed {
  button: QuickButton;
  cx: number;
  cy: number;
}

export function placeButtons(buttons: QuickButton[], size: Size): Placed[] {
  const s = shoulder(size.height);
  const body = { minX: s, maxX: size.width - s, maxY: size.height, midX: size.width / 2, midY: size.height / 2 };
  const left = buttons.filter((b) => b.side === "left").slice(0, 3);
  const right = buttons.filter((b) => b.side === "right").slice(0, 3);
  const bottom = buttons.filter((b) => b.side === "bottom").slice(0, 3);
  const span = (Math.max(left.length, right.length, 1) - 1) * ROW;
  const sideTop = Math.max(22 + 12, Math.min(52, body.midY - span / 2));
  return [
    ...left.map((button, i) => ({ button, cx: body.minX - OFFSET, cy: sideTop + i * ROW })),
    ...right.map((button, i) => ({ button, cx: body.maxX + OFFSET, cy: sideTop + i * ROW })),
    ...bottom.map((button, i) => ({
      button,
      cx: body.midX + (i - (bottom.length - 1) / 2) * ROW,
      cy: body.maxY + OFFSET,
    })),
  ];
}

export function QuickAccess(props: { geometry: Geometry; size: Size; visible: boolean; delay: number }) {
  const settings = useIsland((s) => s.settings);
  const modules = modulesOf({ settings });
  const shown = settings.quickAccess.filter((b) => b.action.kind !== "module" || modules.includes(b.action.module));
  const placed = placeButtons(shown, props.size);

  // The circles take the pointer while they show; the gaps between them do not.
  useEffect(() => {
    if (!props.visible) {
      extraHits.current = [];
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
    return () => {
      extraHits.current = [];
    };
  }, [props.visible, props.size.width, props.size.height, placed.length]);

  return (
    <div
      className="n-quick"
      style={{ width: props.size.width, height: props.size.height, top: props.geometry.floatingDrop }}
    >
      {placed.map((p) => (
        <Bubble key={p.button.id} placed={p} visible={props.visible} delay={props.delay} />
      ))}
    </div>
  );
}

function Bubble({ placed, visible, delay }: { placed: Placed; visible: boolean; delay: number }) {
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
    <motion.button
      type="button"
      className={`n-bubble${isSelected ? " is-selected" : ""}`}
      aria-label={label}
      title={label}
      style={{ left: placed.cx - DIAMETER / 2, top: placed.cy - DIAMETER / 2 }}
      initial={{ scale: 0.4, opacity: 0 }}
      animate={
        visible
          ? { scale: 1, opacity: 1, transition: { ...REVEAL, delay } }
          : { scale: 0.4, opacity: 0, transition: { duration: 0.16, ease: "easeIn" } }
      }
      onClick={run}
      tabIndex={visible ? 0 : -1}
    >
      <Icon size={19} strokeWidth={1.75} />
    </motion.button>
  );
}
