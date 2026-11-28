// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../../i18n";

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useContext, useId, useRef } from "react";

import { activityPickerLayout, activitySize, capsuleBody, shoulder, type Geometry } from "../geometry";
import { moduleInfo } from "../modules";
import type { Size } from "../motion";
import { compactActivities, useIsland, visibleActivity, type ActivityId } from "../store";
import { DrawnSize } from "../surface/DrawnSize";
import { ActivityStrip } from "./Activity";
import { MascotActivityVisit } from "../../mascot/MascotActivityVisit";
import { countItems } from "../../sessions/SessionsStrip";
import { presentedSnapshot } from "../../sessions/presentation";
import { useSessions } from "../../sessions/store";

/** Compact activities keep their own controls while stacking them along a side edge. */
export function fitPortraitSize(size: Size, screen: Size): Size {
  const height = Math.max(1, Math.min(size.height, screen.height - 24));
  return { width: Math.max(1, Math.min(size.width, screen.width - 24, height - 8)), height };
}

export function portraitActivitySize(size: Size, screen: Size, id?: ActivityId): Size {
  const minimum = id === "sessions" ? 40 + 24 * countItems(presentedSnapshot(useSessions.getState()).counts).length : 0;
  return fitPortraitSize({ width: size.height, height: Math.max(size.width, minimum) }, screen);
}

export function portraitPickerSize(strips: Size[], screen: Size, ids: ActivityId[] = []): Size {
  const stripHeight = Math.max(0, ...strips.map((size, i) => portraitActivitySize(size, screen, ids[i]).height));
  const stripWidth = Math.max(0, ...strips.map((size) => size.height));
  const width = stripWidth + 10 + 104 + 2 * (shoulder(screen.height) + 8);
  return fitPortraitSize({ width, height: Math.max(width + 8, stripHeight, 24 + strips.length * 36 + Math.max(0, strips.length - 1) * 10) }, screen);
}

/** Keep one strip mounted while the choices and the surrounding surface open or close. */
export function ActivityPicker({ geometry, size, visible, vertical = false, edge = "left" }: { geometry: Geometry; size: Size; visible: boolean; vertical?: boolean; edge?: "left" | "right" }) {
  useLocale();
  const state = useIsland();
  const choices = compactActivities(state);
  const active = visibleActivity(state);
  const selected = visible ? (active && choices.includes(active) ? active : choices[0]) : active;
  const highlight = useId();
  const buttons = useRef(new Map<ActivityId, HTMLButtonElement>());
  const reduce = useReducedMotion();
  const drawn = useContext(DrawnSize) ?? size;
  const closingPicker = useRef(false);
  if (!selected) return null;
  const strips = choices.map((id) =>
    activitySize(geometry, state.activities[id]!.wing, state.activities[id]!.capsuleContent),
  );
  const activity = state.activities[selected]!;
  const strip = vertical ? portraitActivitySize(activitySize(geometry, activity.wing, activity.capsuleContent), geometry.screen, selected) : activitySize(geometry, activity.wing, activity.capsuleContent);
  // The side strip must follow the surface's drawn size, not the menu's final
  // width. A flex row at the final width pushed it outside the growing capsule.
  const pickerSize = vertical ? portraitPickerSize(strips, geometry.screen, choices) : size;
  const expansion = vertical
    ? Math.max(0, Math.min(1, (drawn.width - strip.width) / Math.max(1, pickerSize.width - strip.width)))
    : 1;
  const layout = vertical
    ? { size: drawn, headerHeight: drawn.height, horizontal: 0, columns: 1 }
    : activityPickerLayout(geometry, strips);
  const menuOpacity = Math.max(0, Math.min(1, (expansion - 0.82) / 0.18));
  const fullSidePadding = vertical ? capsuleBody(pickerSize.width, pickerSize.height, geometry.floatingGap ?? 0).x + 8 : 0;
  const sidePadding = fullSidePadding * expansion;
  const headerLeft = edge === "right" ? drawn.width - strip.width - sidePadding : sidePadding;
  const menuHeight = Math.min(choices.length * 36 + Math.max(0, choices.length - 1) * 10, Math.max(1, pickerSize.height - 24));
  const menuLeft = edge === "right"
    ? drawn.width - strip.width - fullSidePadding - 114
    : fullSidePadding + strip.width + 10;
  // Keep the header at its natural size while the taller chooser closes.
  // Hover, however, rides the surface's actual springs on both axes instead
  // of running a second animation toward its destination size.
  if (visible) closingPicker.current = true;
  else if (Math.abs(drawn.width - size.width) < 0.5 && Math.abs(drawn.height - size.height) < 0.5)
    closingPicker.current = false;
  const naturalSize = visible || closingPicker.current;
  const scaleX = vertical || naturalSize ? 1 : drawn.width / strip.width;
  const scaleY = vertical || naturalSize ? 1 : drawn.height / strip.height;
  return (
    <div
      className={`n-compact-activity${visible ? " is-choosing" : ""}${vertical ? " is-vertical" : ""}`}
      style={{ width: layout.size.width, height: layout.size.height }}
    >
      <div className="n-activity-picker-header" style={vertical ? {
        width: strip.width, height: strip.height, left: headerLeft, top: Math.max(0, (drawn.height - strip.height) / 2),
      } : { height: layout.headerHeight }}
        onClick={(event) => {
          if (!visible && !(event.target as Element).closest(".n-activity-picker-strip,button,[role=button]"))
            useIsland.getState().clickClosed();
        }}>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={selected}
            className="n-activity-picker-strip"
            style={{
              width: strip.width,
              height: strip.height,
              left: vertical ? 0 : `calc(50% - ${strip.width / 2}px)`,
              top: 0,
            }}
            initial={{ opacity: 0, filter: reduce ? "blur(0px)" : "blur(4px)" }}
            animate={{ opacity: 1, filter: "blur(0px)" }}
            exit={{ opacity: 0, filter: reduce ? "blur(0px)" : "blur(4px)" }}
            transition={{ duration: reduce ? 0 : 0.12 }}
          >
            <div style={{ transform: `scale(${scaleX}, ${scaleY})`, transformOrigin: "50% 0" }}>
              <DrawnSize.Provider value={strip}>
                <MascotActivityVisit geometry={geometry} strip={strip}>
                  <div className={vertical ? "n-activity-portrait" : undefined}>
                    <ActivityStrip id={selected} geometry={geometry} size={strip} />
                  </div>
                </MascotActivityVisit>
              </DrawnSize.Provider>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
      <AnimatePresence initial={false}>
        {visible || vertical ? (
          <motion.div
            className="n-activity-picker"
            key="choices"
            initial={vertical ? false : { opacity: 0, y: reduce ? 0 : -4 }}
            animate={vertical ? { opacity: menuOpacity, x: 0, y: 0 } : { opacity: 1, y: 0 }}
            exit={vertical ? undefined : { opacity: 0, y: reduce ? 0 : -4 }}
            transition={{ duration: vertical || reduce ? 0 : 0.12 }}
            style={vertical ? {
              width: 104, height: menuHeight, left: menuLeft, top: (drawn.height - menuHeight) / 2,
              pointerEvents: visible && expansion > 0.82 ? "auto" : "none",
            } : undefined}
          >
            <div
              className="n-activity-picker-choices"
              role="group"
              aria-label={t("island.activities")}
              style={{
                paddingInline: layout.horizontal,
                gridTemplateColumns: `repeat(${layout.columns},minmax(0,1fr))`,
              }}
              onKeyDown={(event) => {
                let next = choices.indexOf(selected);
                if (event.key === "ArrowRight") next = (next + 1) % choices.length;
                else if (event.key === "ArrowLeft") next = (next + choices.length - 1) % choices.length;
                else if (event.key === "Home") next = 0;
                else if (event.key === "End") next = choices.length - 1;
                else return;
                event.preventDefault();
                const choice = choices[next]!;
                state.selectCompactActivity(choice);
                buttons.current.get(choice)?.focus({ preventScroll: true });
              }}
            >
              {choices.map((id) => {
                const module = state.activities[id]!.opens;
                const Icon = moduleInfo(module).icon;
                const label = id === "music" ? t("activity.music") : t(`module.${module}`);
                return (
                  <button
                    key={id}
                    type="button"
                    className={`n-activity-choice${id === selected ? " is-selected" : ""}`}
                    aria-pressed={id === selected}
                    aria-label={label}
                    title={label}
                    ref={(element) => {
                      if (element) buttons.current.set(id, element);
                      else buttons.current.delete(id);
                    }}
                    onClick={() => {
                      state.selectCompactActivity(id);
                      if (vertical) useIsland.getState().open(module);
                    }}
                  >
                    {id === selected ? (
                      <motion.span
                        className="n-activity-choice-highlight"
                        layoutId={vertical ? undefined : highlight}
                        transition={{ duration: reduce ? 0 : 0.22, ease: [0.16, 1, 0.3, 1] }}
                      />
                    ) : null}
                    <Icon size={14} strokeWidth={1.8} aria-hidden="true" />
                    <span>{label}</span>
                  </button>
                );
              })}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
