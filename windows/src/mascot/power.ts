// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Energy saver, the Mac's Low Power Mode, for every window that draws the
// companion: it blinks half as often, and takes no strolls of its own.

import { useSyncExternalStore } from "react";
import { bridge } from "../shared/bridge";

let lowPower = false;
const listeners = new Set<() => void>();

function set(on: boolean) {
  if (lowPower === on) return;
  lowPower = on;
  listeners.forEach((listener) => listener());
}

// The event is heard before the first reading is asked for, and once one is
// heard the reading, which may answer later, is older and ignored.
let heard = false;
void bridge.power
  .onSaver((on) => {
    heard = true;
    set(on);
  })
  .then(() => bridge.power.saver())
  .then((on) => on !== undefined && !heard && set(on))
  .catch(() => {});

export const mascotLowPower = () => lowPower;

export function onMascotLowPower(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

export const useMascotLowPower = () => useSyncExternalStore(onMascotLowPower, mascotLowPower);
