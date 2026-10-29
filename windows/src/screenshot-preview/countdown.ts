// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** A paused preview keeps the remaining time, including across repeated hovers.
 * Pinning/zero duration disable expiry. The owner disposes on window unmount. */
export class PreviewCountdown {
  private remaining: number;
  private started = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;

  constructor(
    seconds: number,
    private readonly expire: () => void,
  ) {
    this.remaining = seconds > 0 ? seconds * 1000 : Infinity;
  }

  update(paused: boolean, pinned: boolean) {
    if (this.disposed) return;
    if (this.timer !== undefined) {
      clearTimeout(this.timer);
      this.remaining = Math.max(0, this.remaining - (performance.now() - this.started));
      this.timer = undefined;
    }
    if (paused || pinned || !Number.isFinite(this.remaining)) return;
    this.started = performance.now();
    this.timer = setTimeout(() => {
      this.timer = undefined;
      if (!this.disposed) {
        this.disposed = true;
        this.expire();
      }
    }, this.remaining);
  }

  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
  }
}
