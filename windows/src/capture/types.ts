// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** The tools the controls switch between; each is also a session's purpose. */
export type CaptureTool = "screenshot" | "recording" | "text" | "color";

/** What the overlay draws (capture/mod.rs SessionInfo). */
export interface CaptureSession {
  id: number;
  purpose: "screenshot" | "recording" | "text" | "color" | "watch";
  /** The frozen picture, in the display's physical pixels. */
  width: number;
  height: number;
  colorFormat: "hex" | "rgb" | "hsl";
  bareHex: boolean;
  /** The last area taken on this display. */
  lastArea?: { x: number; y: number; width: number; height: number } | null;
}

/** The person's choice, in the frozen picture's pixels. */
export type Pick =
  { kind: "area"; x: number; y: number; width: number; height: number } | { kind: "point"; x: number; y: number };
