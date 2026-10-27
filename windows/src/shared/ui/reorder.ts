// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Reordering by pointer. The island window takes file drops, which turns off
// HTML5 drag and drop inside WebView2 (spec 05 Open question 3), so columns,
// tiles and tabs move with pointer events instead.

import { useRef, useState } from "react";

/** Where `id` lands when dropped over the item whose rect holds the point. */
export function reorderedIds<T>(ids: readonly T[], moving: T, target: T, after: boolean): T[] {
  if (moving === target) return [...ids];
  const rest = ids.filter((id) => id !== moving);
  const at = rest.indexOf(target);
  if (at < 0) return [...ids];
  rest.splice(after ? at + 1 : at, 0, moving);
  return rest;
}

/**
 * Items carry `data-reorder-id`. Dragging more than 4 px starts a move;
 * `allowed(moving, target)` limits it (pinned and unpinned groups).
 */
export function useReorder<T extends string>(opts: {
  ids: readonly T[];
  axis: "x" | "y" | "grid";
  onReorder(ids: T[]): void;
  allowed?(moving: T, target: T): boolean;
  /** Only start with a modifier (the Mac's Command-drag in the mixer). */
  requireModifier?: boolean;
}) {
  const [moving, setMoving] = useState<T | null>(null);
  const start = useRef<{ id: T; x: number; y: number; pointer: number; el: Element } | null>(null);

  const targetAt = (x: number, y: number): { id: T; after: boolean } | null => {
    const el = document.elementFromPoint(x, y)?.closest("[data-reorder-id]");
    if (!el) return null;
    const id = el.getAttribute("data-reorder-id") as T;
    const r = el.getBoundingClientRect();
    const after = opts.axis === "y" ? y > r.top + r.height / 2 : x > r.left + r.width / 2;
    return { id, after };
  };

  return {
    moving,
    itemProps(id: T) {
      return {
        "data-reorder-id": id,
        onPointerDown(e: React.PointerEvent) {
          if (e.button !== 0) return;
          if (opts.requireModifier && !(e.ctrlKey || e.altKey)) return;
          start.current = { id, x: e.clientX, y: e.clientY, pointer: e.pointerId, el: e.currentTarget };
        },
        onPointerMove(e: React.PointerEvent) {
          const s = start.current;
          if (!s || s.pointer !== e.pointerId) return;
          if (moving === null) {
            if (Math.hypot(e.clientX - s.x, e.clientY - s.y) < 4) return;
            (s.el as HTMLElement).setPointerCapture(e.pointerId);
            setMoving(s.id);
          }
          const target = targetAt(e.clientX, e.clientY);
          if (!target || target.id === s.id) return;
          if (opts.allowed && !opts.allowed(s.id, target.id)) return;
          const next = reorderedIds(opts.ids, s.id, target.id, target.after);
          if (next.join("\u0000") !== opts.ids.join("\u0000")) opts.onReorder(next);
        },
        onPointerUp(e: React.PointerEvent) {
          if (start.current?.pointer === e.pointerId) {
            if (moving !== null) e.preventDefault();
            start.current = null;
            setMoving(null);
          }
        },
        onPointerCancel() {
          start.current = null;
          setMoving(null);
        },
        /** Swallow the click that ends a move. */
        onClickCapture(e: React.MouseEvent) {
          if (moving !== null) {
            e.stopPropagation();
            e.preventDefault();
          }
        },
      };
    },
  };
}
