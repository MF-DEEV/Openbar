// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The sessions' tray icon (#331), drawn here and handed to tray.rs as PNG:
// Openbar's Sessions mark (AgentFace.ts) in white on a dark rounded square,
// which reads on light and dark taskbars alike without asking which one it
// sits on, with an orange dot while a session waits and the count in it
// from two. A second picture with the dot swollen makes the pulse.

import { AGENT_FACE_PATH } from "../shared/ui/AgentFace";
import { badgeText } from "./trayState";

/** The design grid; every measure below is on it. */
const GRID = 32;
const SURFACE = "#18181b";
const EDGE = "rgba(255, 255, 255, 0.22)";
const DOT = "#ff9f0a";
const DOT_TEXT = "#1c1205";

/** The dot's place and size on the grid, larger to hold a count. */
export function dotGeometry(waiting: number, swollen: boolean): { x: number; y: number; r: number } | null {
  if (waiting <= 0) return null;
  const r = (badgeText(waiting) ? 7.6 : 5.6) * (swollen ? 1.2 : 1);
  return { x: 24, y: 8, r };
}

/** The picture's size in pixels: Windows' small icon at the screen's scale, Linux's panels scale it down. */
export function iconSize(linux: boolean, scale: number): number {
  if (linux) return 48;
  return Math.min(64, Math.max(16, Math.round(16 * (Number.isFinite(scale) && scale > 0 ? scale : 1))));
}

async function fontReady(size: number) {
  try {
    await document.fonts?.load(`700 ${size}px "Inter Variable"`);
  } catch {
    // The system face draws the count instead.
  }
}

/** The PNG bytes of one picture. */
export async function renderTrayIcon(waiting: number, swollen: boolean, size: number): Promise<number[]> {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const context = canvas.getContext("2d");
  if (!context) return [];
  const unit = size / GRID;
  context.scale(unit, unit);

  // The square, with a faint edge so it stands apart from a dark taskbar.
  context.beginPath();
  context.roundRect(1.5, 1.5, 29, 29, 8);
  context.fillStyle = SURFACE;
  context.fill();
  context.lineWidth = 1;
  context.strokeStyle = EDGE;
  context.stroke();

  // The face: the 24-unit mark at 22 units, in the square's middle.
  context.save();
  context.translate(5, 5.5);
  context.scale(22 / 24, 22 / 24);
  context.fillStyle = "#ffffff";
  context.fill(new Path2D(AGENT_FACE_PATH), "evenodd");
  context.restore();

  const dot = dotGeometry(waiting, swollen);
  if (dot) {
    // A ring of the square's colour parts the dot from the face behind it.
    context.beginPath();
    context.arc(dot.x, dot.y, dot.r + 1.6, 0, Math.PI * 2);
    context.fillStyle = SURFACE;
    context.fill();
    context.beginPath();
    context.arc(dot.x, dot.y, dot.r, 0, Math.PI * 2);
    context.fillStyle = DOT;
    context.fill();
    const text = badgeText(waiting);
    if (text) {
      const fontSize = text.length > 1 ? 8.6 : 10.5;
      await fontReady(Math.round(fontSize * unit));
      context.font = `700 ${fontSize}px "Inter Variable", "Segoe UI", system-ui, sans-serif`;
      context.fillStyle = DOT_TEXT;
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillText(text, dot.x, dot.y + 0.6);
    }
  }

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) return [];
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}
