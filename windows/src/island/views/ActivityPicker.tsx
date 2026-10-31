// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../../i18n";

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useContext, useId, useRef } from "react";

import { activityPickerLayout, activitySize, type Geometry } from "../geometry";
import { moduleInfo } from "../modules";
import type { Size } from "../motion";
import { compactActivities, useIsland, visibleActivity, type ActivityId } from "../store";
import { DrawnSize } from "../surface/DrawnSize";
import { ActivityStrip } from "./Activity";
import { MascotActivityVisit } from "../../mascot/MascotActivityVisit";

/** Keep one strip mounted while the choices and the surrounding surface open or close. */
export function ActivityPicker({ geometry, size, visible }: { geometry: Geometry; size: Size; visible: boolean }) {
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
  const layout = activityPickerLayout(geometry, strips);
  const activity = state.activities[selected]!;
  const strip = activitySize(geometry, activity.wing, activity.capsuleContent);
  // Keep the header at its natural size while the taller chooser closes.
  // Hover, however, rides the surface's actual springs on both axes instead
  // of running a second animation toward its destination size.
  if (visible) closingPicker.current = true;
  else if (Math.abs(drawn.width - size.width) < 0.5 && Math.abs(drawn.height - size.height) < 0.5)
    closingPicker.current = false;
  const naturalSize = visible || closingPicker.current;
  const scaleX = naturalSize ? 1 : drawn.width / strip.width;
  const scaleY = naturalSize ? 1 : drawn.height / strip.height;
  return (
    <div
      className={`n-compact-activity${visible ? " is-choosing" : ""}`}
      style={{ width: layout.size.width, height: layout.size.height }}
    >
      <div className="n-activity-picker-header" style={{ height: layout.headerHeight }}>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={selected}
            className="n-activity-picker-strip"
            style={{
              width: strip.width,
              height: strip.height,
              left: `calc(50% - ${strip.width / 2}px)`,
            }}
            initial={{ opacity: 0, filter: reduce ? "blur(0px)" : "blur(4px)" }}
            animate={{ opacity: 1, filter: "blur(0px)" }}
            exit={{ opacity: 0, filter: reduce ? "blur(0px)" : "blur(4px)" }}
            transition={{ duration: reduce ? 0 : 0.12 }}
          >
            <div style={{ transform: `scale(${scaleX}, ${scaleY})`, transformOrigin: "50% 0" }}>
              <DrawnSize.Provider value={strip}>
                <MascotActivityVisit geometry={geometry} strip={strip}>
                  <ActivityStrip id={selected} geometry={geometry} size={strip} />
                </MascotActivityVisit>
              </DrawnSize.Provider>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
      <AnimatePresence initial={false}>
        {visible ? (
          <motion.div
            className="n-activity-picker"
            key="choices"
            initial={{ opacity: 0, y: reduce ? 0 : -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: reduce ? 0 : -4 }}
            transition={{ duration: reduce ? 0 : 0.12 }}
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
                    onClick={() => state.selectCompactActivity(id)}
                  >
                    {id === selected ? (
                      <motion.span
                        className="n-activity-choice-highlight"
                        layoutId={highlight}
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
