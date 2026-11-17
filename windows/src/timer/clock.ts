// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The timer's continuous clock, in seconds. The Mac anchors on
// ContinuousClock, which counts through sleep (NotchTimerService.swift:12-23).
// performance.now() is the webview's monotonic clock, but whether it counts
// a system sleep depends on the platform's tick source, so it is checked
// against Windows' interrupt time (timer_now_ms, which includes sleep): what
// the interrupt time gained beyond performance.now() is added as skew. The
// first reading only sets the base, so anchors taken before it stay valid,
// and the skew only ever grows, so the clock never runs backwards.

import { bridge } from "../shared/bridge";

/** Below this the two clocks disagree only by IPC latency and tick granularity. */
const SKEW_NOISE_MS = 250;
/** A wall-clock jump this much beyond performance.now() means a sleep (or a clock change): check again. */
const JUMP_MS = 2000;

export interface ClockSource {
  perf(): number;
  wall(): number;
  ticks(): Promise<number | undefined>;
}

export function makeClock(source: ClockSource) {
  let base: { perf: number; ticks: number } | null = null;
  let skew = 0;
  let last = { perf: source.perf(), wall: source.wall() };
  return {
    /** Seconds on the continuous clock. */
    now: () => (source.perf() + skew) / 1000,
    /** Measures the interrupt time again; resolves once any uncounted sleep is added. */
    async resync(): Promise<void> {
      const before = source.perf();
      const ticks = await source.ticks();
      const after = source.perf();
      if (ticks === undefined || !Number.isFinite(ticks)) return;
      const perf = (before + after) / 2;
      if (!base) {
        base = { perf, ticks };
        return;
      }
      const measured = ticks - base.ticks - (perf - base.perf);
      if (measured - skew > SKEW_NOISE_MS) skew = measured;
    },
    /** True once per wall-clock jump that performance.now() did not see. */
    jumped(): boolean {
      const now = { perf: source.perf(), wall: source.wall() };
      const drift = now.wall - last.wall - (now.perf - last.perf);
      last = now;
      return Math.abs(drift) > JUMP_MS;
    },
  };
}

export const clock = makeClock({
  perf: () => performance.now(),
  wall: () => Date.now(),
  ticks: () => bridge.timer.now(),
});
