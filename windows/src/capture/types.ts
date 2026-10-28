// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** What the overlay draws (capture/mod.rs SessionInfo). */
export interface CaptureSession {
  id: number;
  purpose: "screenshot" | "text" | "color" | "watch";
  /** The frozen picture, in the display's physical pixels. */
  width: number;
  height: number;
  colorFormat: "hex" | "rgb" | "hsl";
  bareHex: boolean;
}

/** The person's choice, in the frozen picture's pixels. */
export type Pick =
  { kind: "area"; x: number; y: number; width: number; height: number } | { kind: "point"; x: number; y: number };
