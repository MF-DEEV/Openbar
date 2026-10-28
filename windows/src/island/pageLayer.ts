// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Inner layers of a page (mixer options, clipboard search, a month grid, an
// open menu). Escape closes the newest layer before it steps the island back
// (NotchService.setPageLayer / stepBack, spec 05 §0).

import { useEffect, useRef } from "react";

const layers: { id: number; close: () => void }[] = [];
let nextId = 0;

/** Closes the newest open layer; false when none is open. */
export function closeTopLayer(): boolean {
  const top = layers.pop();
  if (!top) return false;
  top.close();
  return true;
}

export function openLayerCount(): number {
  return layers.length;
}

/** While `open`, Escape runs `close` first. */
export function usePageLayer(open: boolean, close: () => void) {
  const latest = useRef(close);
  latest.current = close;
  useEffect(() => {
    if (!open) return;
    const id = nextId++;
    layers.push({ id, close: () => latest.current() });
    return () => {
      const i = layers.findIndex((l) => l.id === id);
      if (i >= 0) layers.splice(i, 1);
    };
  }, [open]);
}
