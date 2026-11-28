// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's content and its transitions (NotchWindowHost.swift; spec
// 01-shell §3.2). Expanded content is laid out at its final size; only its
// opacity, blur and scale change here, never the black backdrop.

import { useLocale } from "../../i18n";

import { type ReactNode, useContext, useLayoutEffect, useRef, useState } from "react";
import type { ContentTransition } from "../store";
import { DrawnSize } from "./DrawnSize";
import { surfaceOffset, type IslandEdge, type IslandPosition } from "../placement";

interface Layer {
  /** Stable for the layer's life, so turning to leave does not remount it. */
  id: number;
  key: string;
  node: ReactNode;
  width: number;
  height: number;
  leaving: boolean;
}

const EASE_OUT = "cubic-bezier(0, 0, 0.58, 1)";
const EASE_IN = "cubic-bezier(0.42, 0, 1, 1)";
const EASE_IN_OUT = "cubic-bezier(0.42, 0, 0.58, 1)";

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

function animateIn(el: HTMLElement, kind: ContentTransition, side = false) {
  if (reducedMotion() && kind !== "none") {
    el.animate([{ opacity: 0 }, { opacity: 0, offset: 0.25 }, { opacity: 1 }], { duration: 200 });
    return;
  }
  switch (kind) {
    case "reveal":
      // [0, 0, 1] at [0, 0.35, 1] of 450 ms; blur 12 → 6 → 0; scale 0.86 → 1 from the top centre.
      el.animate(
        [
          { opacity: 0, filter: "blur(12px)", easing: "linear" },
          { opacity: 0, filter: "blur(6px)", offset: 0.35, easing: EASE_OUT },
          { opacity: 1, filter: "blur(0px)" },
        ],
        { duration: 450 },
      );
      if (!side) el.animate([{ transform: "scale(0.86)" }, { transform: "scale(1)" }], {
        duration: 450,
        easing: "cubic-bezier(0.22, 1, 0.36, 1)",
      });
      break;
    case "dismiss":
      el.animate(
        [
          { opacity: 0, filter: "blur(6px)" },
          { opacity: 0, filter: "blur(6px)", offset: 0.65 },
          { opacity: 1, filter: "blur(0px)" },
        ],
        { duration: 400 },
      );
      break;
    case "replace":
      el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 180, easing: EASE_IN_OUT });
      break;
    case "depart":
      // Swapped in at 200 ms, fading in over 140 ms from 260 ms.
      el.animate([{ opacity: 0 }, { opacity: 0, offset: 260 / 400 }, { opacity: 1 }], {
        duration: 400,
        easing: EASE_OUT,
      });
      break;
    case "none":
      break;
  }
}

function animateOut(el: HTMLElement, kind: ContentTransition, side = false): number {
  if (side && (kind === "reveal" || kind === "dismiss")) {
    el.animate(
      kind === "reveal"
        ? [{ opacity: 1 }, { opacity: 1, offset: 0.28 }, { opacity: 0, offset: 0.56 }, { opacity: 0 }]
        : [{ opacity: 1 }, { opacity: 0, offset: 0.65 }, { opacity: 0 }],
      { duration: 400, fill: "forwards" },
    );
    return 400;
  }
  if (kind === "replace") {
    el.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 180, easing: EASE_IN_OUT, fill: "forwards" });
    return 180;
  }
  if (kind === "depart") {
    el.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 160, easing: EASE_IN, fill: "forwards" });
    return 200;
  }
  return 0;
}

export function ContentStage(props: {
  contentKey: string;
  transition: ContentTransition;
  width: number;
  height: number;
  edge?: IslandEdge;
  position?: IslandPosition;
  children: ReactNode;
}) {
  useLocale();
  const { contentKey, transition, width, height, children } = props;
  const side = props.edge === "left" || props.edge === "right";
  const drawn = useContext(DrawnSize);
  const nextId = useRef(1);
  const [layers, setLayers] = useState<Layer[]>([
    { id: 0, key: contentKey, node: children, width, height, leaving: false },
  ]);
  const refs = useRef(new Map<number, HTMLDivElement>());
  const lastKey = useRef(contentKey);
  const pendingIn = useRef<{ id: number; kind: ContentTransition } | null>(null);

  // A new key: the old layer leaves (or goes at once) and the new one enters.
  if (contentKey !== lastKey.current) {
    const previous = lastKey.current;
    lastKey.current = contentKey;
    const id = nextId.current++;
    pendingIn.current = { id, kind: transition };
    const keepsOld = transition === "replace" || transition === "depart" || (side && (transition === "reveal" || transition === "dismiss"));
    setLayers((current) => [
      ...current.filter((l) => keepsOld && l.key === previous && !l.leaving).map((l) => ({ ...l, leaving: true })),
      { id, key: contentKey, node: children, width, height, leaving: false },
    ]);
  }

  // The same key: keep the entering layer's content and size current.
  const top = layers.at(-1);
  if (
    top &&
    top.key === contentKey &&
    !top.leaving &&
    (top.node !== children || top.width !== width || top.height !== height)
  ) {
    setLayers((current) =>
      current.map((l) => (l.key === contentKey && !l.leaving ? { ...l, node: children, width, height } : l)),
    );
  }

  useLayoutEffect(() => {
    const pending = pendingIn.current;
    if (!pending) return;
    pendingIn.current = null;
    const entering = refs.current.get(pending.id);
    if (entering) animateIn(entering, pending.kind, side);
    for (const layer of layers) {
      if (!layer.leaving) continue;
      const el = refs.current.get(layer.id);
      const wait = el ? animateOut(el, pending.kind, side) : 0;
      setTimeout(() => setLayers((current) => current.filter((l) => l.id !== layer.id)), wait);
    }
  }, [layers]);

  return (
    <>
      {layers.map((layer) => {
        // Compact views share the surface's live dimensions, including their
        // centering. Expanded pages keep a stable layout during reveal.
        const size = !layer.leaving && !layer.key.startsWith("expanded:") && drawn ? drawn : layer;
        const offset = surfaceOffset({ width, height }, size, props.edge ?? "top", props.position);
        return (
          <div
            key={layer.id}
            ref={(el) => {
              if (el) refs.current.set(layer.id, el);
              else refs.current.delete(layer.id);
            }}
            className="island-layer"
            style={{
              width: size.width,
              height: size.height,
              left: offset.x,
              top: offset.y,
              pointerEvents: layer.leaving ? "none" : undefined,
              transformOrigin: props.edge === "bottom" ? `${(props.position?.x ?? 0.5) * 100}% 100%`
                : props.edge === "left" ? `0 ${(props.position?.y ?? 0.5) * 100}%`
                  : props.edge === "right" ? `100% ${(props.position?.y ?? 0.5) * 100}%`
                    : `${(props.position?.x ?? 0.5) * 100}% 0`,
            }}
          >
            <DrawnSize.Provider value={size}>{layer.node}</DrawnSize.Provider>
          </div>
        );
      })}
    </>
  );
}
