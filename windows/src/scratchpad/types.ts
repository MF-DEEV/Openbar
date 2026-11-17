// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** One pad (ScratchpadPad, ScratchpadSupport.swift:57-62). */
export interface Pad {
  id: string;
  name: string;
  text: string;
  /** Milliseconds since 1970 of the last edit; absent while empty. */
  modifiedAt?: number;
}

/** The whole scratchpad as stored (ScratchpadDocument); array order is tab order. */
export interface ScratchpadDocument {
  pads: Pad[];
  selectedId: string;
}
