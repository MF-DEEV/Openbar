// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { bridge } from "../shared/bridge";

export interface AudioLevels {
  source: string;
  mode: "process" | "system" | "unavailable" | "inactive";
  levels: number[];
}

type Subscriber = (frame: AudioLevels | null) => void;
const subscribers = new Set<Subscriber>();
let stop: (() => void) | undefined;
let commands: Promise<unknown> = Promise.resolve();

function watch(on: boolean) {
  // Preserve order across React effect cleanup, remounts and slow IPC calls.
  commands = commands.then(() => bridge.music.watchLevels(on)).catch(() => {});
}

/** A single listener and lease shared by all visible equalizers in this webview. */
export function subscribeAudioLevels(subscriber: Subscriber): () => void {
  subscribers.add(subscriber);
  if (!stop) {
    let disposed = false;
    let listening = false;
    let stale: ReturnType<typeof setTimeout> | undefined;
    const deliver = (frame: AudioLevels | null) => {
      for (const receive of subscribers) receive(frame);
    };
    const off = bridge.music.onLevels((frame) => {
      if (disposed) return;
      clearTimeout(stale);
      deliver(frame);
      stale = setTimeout(() => deliver(null), 300);
    });
    void off
      .then(() => {
        if (disposed) return;
        listening = true;
        watch(true);
      })
      .catch(() => {
        if (!disposed) deliver(null);
      });
    const heartbeat = setInterval(() => {
      if (!disposed && listening) watch(true);
    }, 5000);
    stop = () => {
      disposed = true;
      clearTimeout(stale);
      clearInterval(heartbeat);
      void off.then((unlisten) => unlisten()).catch(() => {});
      watch(false);
    };
  }
  return () => {
    if (!subscribers.delete(subscriber)) return;
    if (!subscribers.size) {
      stop?.();
      stop = undefined;
    }
  };
}

// Interleave the sampled bands: bass sits inside the meter and treble is
// scattered between it. Keep this order stable so a tone never jumps randomly.
const BAR_ORDER: Record<number, readonly number[]> = {
  3: [1, 0, 2],
  4: [2, 0, 3, 1],
  7: [4, 1, 5, 0, 3, 6, 2],
};

/** Sample the frequency range, then scatter those bands across the bars. */
export function barHeights(count: number, width: number, height: number, levels: readonly number[]): number[] {
  return Array.from({ length: count }, (_, bar) => {
    const position = BAR_ORDER[count]?.[bar] ?? bar;
    const index = Math.min(levels.length - 1, Math.floor(((position + 0.5) / count) * levels.length));
    const raw = levels[index] ?? 0;
    const level = Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0;
    return width + Math.max(0, height - width) * level;
  });
}
