// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// When a compact entry opens over its neighbours (NotchClipboardView.swift:
// 119-125, 295-332; NotchClipboardSizeStrings.swift:26-31): after the pointer
// rests on it 0.7 s, or the arrow keys rest on it 1.4 s. It closes 150 ms
// after the pointer leaves, unless the pointer comes back to that entry;
// resting on another one or leaving the list closes it.

/** How long the pointer rests on a compact entry before it opens. */
export const DWELL_MS = 700;
/** Longer for the keys, which cross many entries on their way. */
export const KEY_DWELL_MS = 1400;
export const CLOSE_MS = 150;

type Timer = ReturnType<typeof setTimeout>;

export class Dwell {
  hovered: string | null = null;
  /** Opened by the pointer. */
  expanded: string | null = null;
  /** Opened by the keys. */
  keyExpanded: string | null = null;
  private dwell: { id: string; timer: Timer } | null = null;
  private closing: { id: string; timer: Timer } | null = null;
  private keyTimer: Timer | undefined;

  constructor(private readonly changed: () => void) {}

  /** The entry that is open, by the pointer or by the keys. */
  get opened(): string | null {
    return this.expanded ?? this.keyExpanded;
  }

  enter(id: string) {
    if (this.closing?.id === id) {
      clearTimeout(this.closing.timer);
      this.closing = null;
    }
    if (this.dwell) clearTimeout(this.dwell.timer);
    this.dwell = null;
    this.hovered = id;
    this.changed();
    if (this.expanded === id) return;
    this.dwell = {
      id,
      timer: setTimeout(() => {
        this.dwell = null;
        if (this.hovered !== id) return;
        this.expanded = id;
        this.changed();
      }, DWELL_MS),
    };
  }

  leave(id: string) {
    if (this.dwell?.id === id) {
      clearTimeout(this.dwell.timer);
      this.dwell = null;
    }
    if (this.hovered === id) {
      this.hovered = null;
      this.changed();
    }
    if (this.expanded !== id) return;
    if (this.closing) clearTimeout(this.closing.timer);
    this.closing = {
      id,
      timer: setTimeout(() => {
        this.closing = null;
        if (this.expanded !== id) return;
        this.expanded = null;
        this.changed();
      }, CLOSE_MS),
    };
  }

  /** The highlight moved: what the keys opened closes, and `id` opens after resting (only once the keys moved). */
  highlight(id: string | null, keysMoved: boolean) {
    clearTimeout(this.keyTimer);
    if (this.keyExpanded !== null) {
      this.keyExpanded = null;
      this.changed();
    }
    if (!keysMoved || id === null) return;
    this.keyTimer = setTimeout(() => {
      this.keyExpanded = id;
      this.changed();
    }, KEY_DWELL_MS);
  }

  dispose() {
    if (this.dwell) clearTimeout(this.dwell.timer);
    if (this.closing) clearTimeout(this.closing.timer);
    clearTimeout(this.keyTimer);
    this.dwell = this.closing = null;
  }
}
