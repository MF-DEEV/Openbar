// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The app's motion vocabulary, built on `motion` and shaped after Animate
// UI's primitives (Highlight, Auto Height, Fade): one highlight that slides
// between choices, groups that grow open on a spring, and pages that settle
// in from the side the user moved toward. The springs match the Mac panel's
// PanelMotion so both apps move alike. Reduced motion keeps the layout
// changes and drops the travel.

import { AnimatePresence, motion, useReducedMotion, type Transition } from "motion/react";
import { type ReactNode, useState } from "react";

/** The tab highlight and the page swap. */
export const SPRING_TAB: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };
/** A selection moving inside a small control. */
export const SPRING_SELECTION: Transition = { type: "spring", stiffness: 520, damping: 38 };
/** A group opening or closing. */
export const SPRING_DISCLOSURE: Transition = { type: "spring", stiffness: 380, damping: 36, restDelta: 0.5 };

const INSTANT: Transition = { duration: 0 };
const QUICK_FADE: Transition = { duration: 0.12, ease: "easeOut" };

/**
 * The one highlight shared by a group of choices. Render it inside the
 * selected choice only; when the selection moves, the highlight travels to
 * the new choice instead of one fading out and another fading in. Give every
 * group its own `layoutId` (useId works).
 */
export function SlidingHighlight(props: { layoutId: string; className: string; transition?: Transition }) {
  const reduce = useReducedMotion();
  return (
    <motion.span
      aria-hidden
      layoutId={props.layoutId}
      className={props.className}
      transition={reduce ? INSTANT : (props.transition ?? SPRING_SELECTION)}
    />
  );
}

/**
 * Content under a disclosure. It grows from nothing to its natural height
 * and fades in, and shrinks back the same way when it closes.
 */
export function Collapse(props: { open: boolean; id?: string; className?: string; children: ReactNode }) {
  const reduce = useReducedMotion();
  // Clipped only while it moves; once open, focus rings at its edges show in
  // full. Starting open counts as settled.
  const [settled, setSettled] = useState(props.open);
  return (
    <AnimatePresence initial={false}>
      {props.open ? (
        <motion.div
          key="body"
          id={props.id}
          className={props.className}
          style={{ overflow: settled ? "visible" : "hidden" }}
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0, overflow: "hidden" }}
          transition={reduce ? QUICK_FADE : SPRING_DISCLOSURE}
          onAnimationStart={() => setSettled(false)}
          // The exit's target has height 0; only the open target settles.
          onAnimationComplete={(target) => setSettled((target as { height?: unknown }).height === "auto")}
        >
          {props.children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * A page that settles in when it mounts: it slides a few pixels in from the
 * side the user moved toward. Key it by the page so each switch plays it
 * once; the outgoing page leaves at once, so the two never stack. It only
 * slides, at full opacity: fading from nothing left the window empty for a
 * couple of frames, and fading from 60% held bright controls visibly dark.
 */
export function PageEnter(props: { direction?: 1 | -1; axis?: "x" | "y"; className?: string; children: ReactNode }) {
  const reduce = useReducedMotion();
  const travel = 10 * (props.direction ?? 1);
  const from = reduce ? {} : { [props.axis ?? "x"]: travel };
  const to = reduce ? {} : { x: 0, y: 0 };
  return (
    <motion.div className={props.className} initial={from} animate={to} transition={reduce ? QUICK_FADE : SPRING_TAB}>
      {props.children}
    </motion.div>
  );
}
