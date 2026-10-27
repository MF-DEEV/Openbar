// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The selection surface over one display (spec 05 §4.2, §15.2): the frozen
// picture taken before this window appeared, dimmed, with the dragged area
// lit and its size beside it; or, picking a colour, the picture as it is
// with a loupe of magnified pixels and the value a click copies. Enter or
// Space takes the whole display, Escape and a right click cancel.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { bridge, inTauri } from "../shared/bridge";
import { formatColor } from "./color";
import { S } from "./strings";
import type { CaptureSession, Pick } from "./types";

interface Point {
  x: number;
  y: number;
}

interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Pixels the loupe shows across, and how big each is drawn. */
const LOUPE_PIXELS = 15;
const LOUPE_CELL = 8;
const LOUPE_SIZE = LOUPE_PIXELS * LOUPE_CELL;
/** A drag smaller than this (in picture pixels) is a slip, not an area. */
const MIN_AREA = 3;

/** Outside the app: a test card the size of this window, for designing. */
function demoSession(): { session: CaptureSession; pixels: Uint8ClampedArray<ArrayBuffer> } {
  const dpr = window.devicePixelRatio || 1;
  const width = Math.round(window.innerWidth * dpr);
  const height = Math.round(window.innerHeight * dpr);
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      const tile = (Math.floor(x / (40 * dpr)) + Math.floor(y / (40 * dpr))) % 2;
      pixels[i] = Math.round((x / width) * 255);
      pixels[i + 1] = tile ? 120 : 160;
      pixels[i + 2] = Math.round((y / height) * 255);
      pixels[i + 3] = 255;
    }
  }
  const purpose = new URLSearchParams(location.search).has("color") ? "color" : "screenshot";
  return { session: { id: 0, purpose, width, height, colorFormat: "hex", bareHex: false }, pixels };
}

function commit(session: CaptureSession, pick: Pick | null) {
  if (inTauri) void bridge.capture.commit(session.id, pick);
  else console.info("capture", pick);
}

const normalized = (a: Point, b: Point): Area => ({
  x: Math.min(a.x, b.x),
  y: Math.min(a.y, b.y),
  width: Math.abs(a.x - b.x),
  height: Math.abs(a.y - b.y),
});

export function Overlay() {
  const [session, setSession] = useState<CaptureSession | null>(null);
  const source = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    void (async () => {
      let loaded: { session: CaptureSession; pixels: Uint8ClampedArray<ArrayBuffer> } | null = null;
      let current: CaptureSession | null | undefined;
      try {
        if (inTauri) {
          current = await bridge.capture.session();
          const buffer = current ? await bridge.capture.pixels(current.id) : undefined;
          if (current && buffer) loaded = { session: current, pixels: new Uint8ClampedArray(buffer) };
        } else {
          loaded = demoSession();
        }
        if (!loaded) throw new Error("no capture");
        const canvas = document.createElement("canvas");
        canvas.width = loaded.session.width;
        canvas.height = loaded.session.height;
        const data = new ImageData(loaded.pixels, loaded.session.width, loaded.session.height);
        canvas.getContext("2d", { willReadFrequently: true })!.putImageData(data, 0, 0);
        source.current = canvas;
        setSession(loaded.session);
      } catch {
        // A picture too big to draw: give the selection up rather than hang it.
        if (current) commit(current, null);
      }
    })();
  }, []);

  if (!session || !source.current) return null;
  return session.purpose === "color" ? (
    <ColorPicker session={session} source={source.current} />
  ) : (
    <AreaPicker session={session} source={source.current} />
  );
}

/** Shows the window once the picture is drawn, so nothing flashes. */
function useReady(session: CaptureSession) {
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (inTauri) void bridge.capture.ready(session.id);
    });
    return () => cancelAnimationFrame(frame);
  }, [session.id]);
}

/**
 * CSS pixels to picture pixels: the window spans the display exactly. A
 * pixel under the pointer stops at the last one; an area's edge may reach
 * past it, so a drag to the corner takes the last row and column too.
 */
function toPicture(session: CaptureSession, clientX: number, clientY: number, edge = false): Point {
  const x = (clientX * session.width) / window.innerWidth;
  const y = (clientY * session.height) / window.innerHeight;
  const clamp = (v: number, max: number) => Math.min(max, Math.max(0, edge ? Math.round(v) : Math.floor(v)));
  return edge
    ? { x: clamp(x, session.width), y: clamp(y, session.height) }
    : { x: clamp(x, session.width - 1), y: clamp(y, session.height - 1) };
}

function useCancelKeys(session: CaptureSession, onKey?: (e: KeyboardEvent) => boolean) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        commit(session, null);
      } else if (onKey?.(e)) {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
}

// MARK: Area

function AreaPicker({ session, source }: { session: CaptureSession; source: HTMLCanvasElement }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [anchor, setAnchor] = useState<Point | null>(null);
  const [area, setArea] = useState<Area | null>(null);
  useReady(session);
  useCancelKeys(session, (e) => {
    if (e.key !== "Enter" && e.key !== " ") return false;
    commit(session, { kind: "area", x: 0, y: 0, width: session.width, height: session.height });
    return true;
  });

  useLayoutEffect(() => {
    const ctx = canvas.current?.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(source, 0, 0);
    ctx.fillStyle = "rgba(0, 0, 0, 0.35)";
    ctx.fillRect(0, 0, session.width, session.height);
    if (!area || area.width === 0 || area.height === 0) return;
    ctx.drawImage(source, area.x, area.y, area.width, area.height, area.x, area.y, area.width, area.height);
    const line = Math.max(1, Math.round(window.devicePixelRatio || 1));
    ctx.strokeStyle = "rgba(255, 255, 255, 0.9)";
    ctx.lineWidth = line;
    ctx.strokeRect(area.x - line / 2, area.y - line / 2, area.width + line, area.height + line);
  }, [area, session, source]);

  const scale = window.innerWidth / session.width;
  const label = area && area.width > 0 && area.height > 0 ? S.size(area.width, area.height) : null;
  // The size sits under the area's bottom-right corner, or inside it near the screen's edge.
  const labelStyle = area
    ? {
        left: Math.min((area.x + area.width) * scale, window.innerWidth - 8),
        top: Math.min((area.y + area.height) * scale + 8, window.innerHeight - 30),
      }
    : undefined;

  return (
    <div
      className="cp-surface is-area"
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        const p = toPicture(session, e.clientX, e.clientY, true);
        setAnchor(p);
        setArea({ ...p, width: 0, height: 0 });
      }}
      onPointerMove={(e) => {
        if (anchor) setArea(normalized(anchor, toPicture(session, e.clientX, e.clientY, true)));
      }}
      onPointerUp={(e) => {
        if (!anchor) return;
        const chosen = normalized(anchor, toPicture(session, e.clientX, e.clientY, true));
        setAnchor(null);
        if (chosen.width >= MIN_AREA && chosen.height >= MIN_AREA) commit(session, { kind: "area", ...chosen });
        else setArea(null);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        commit(session, null);
      }}
    >
      <canvas ref={canvas} className="cp-canvas" width={session.width} height={session.height} />
      {area ? null : (
        <div className="cp-hint" role="status">
          {S.hintDrag} · {S.hintFullScreen} · {S.hintCancel}
        </div>
      )}
      {label ? (
        <div className="cp-size" style={labelStyle}>
          {label}
        </div>
      ) : null}
    </div>
  );
}

// MARK: Colour

function ColorPicker({ session, source }: { session: CaptureSession; source: HTMLCanvasElement }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const loupe = useRef<HTMLCanvasElement>(null);
  const [point, setPoint] = useState<Point | null>(null);
  const pixels = useRef(source.getContext("2d")!);
  useReady(session);
  useCancelKeys(session, (e) => {
    const step = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    if (step && point) {
      setPoint({
        x: Math.min(session.width - 1, Math.max(0, point.x + step[0]!)),
        y: Math.min(session.height - 1, Math.max(0, point.y + step[1]!)),
      });
      return true;
    }
    if ((e.key === "Enter" || e.key === " ") && point) {
      commit(session, { kind: "point", ...point });
      return true;
    }
    return false;
  });

  useLayoutEffect(() => {
    canvas.current?.getContext("2d")?.drawImage(source, 0, 0);
  }, [source]);

  // The loupe: the pixels around the point, magnified, on a grid, the middle one outlined.
  useLayoutEffect(() => {
    const ctx = loupe.current?.getContext("2d");
    if (!ctx || !point) return;
    const dpr = window.devicePixelRatio || 1;
    const half = Math.floor(LOUPE_PIXELS / 2);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, LOUPE_SIZE, LOUPE_SIZE);
    ctx.drawImage(source, point.x - half, point.y - half, LOUPE_PIXELS, LOUPE_PIXELS, 0, 0, LOUPE_SIZE, LOUPE_SIZE);
    ctx.strokeStyle = "rgba(0, 0, 0, 0.18)";
    ctx.lineWidth = 1 / dpr;
    for (let i = 1; i < LOUPE_PIXELS; i++) {
      ctx.beginPath();
      ctx.moveTo(i * LOUPE_CELL, 0);
      ctx.lineTo(i * LOUPE_CELL, LOUPE_SIZE);
      ctx.moveTo(0, i * LOUPE_CELL);
      ctx.lineTo(LOUPE_SIZE, i * LOUPE_CELL);
      ctx.stroke();
    }
    const at = half * LOUPE_CELL;
    ctx.lineWidth = 1;
    ctx.strokeStyle = "#000";
    ctx.strokeRect(at - 0.5, at - 0.5, LOUPE_CELL + 1, LOUPE_CELL + 1);
    ctx.strokeStyle = "#fff";
    ctx.strokeRect(at + 0.5, at + 0.5, LOUPE_CELL - 1, LOUPE_CELL - 1);
  }, [point, source]);

  const [r, g, b] = point ? pixels.current.getImageData(point.x, point.y, 1, 1).data : [0, 0, 0];
  const value = formatColor(r!, g!, b!, session.colorFormat, session.bareHex);
  const hex = formatColor(r!, g!, b!, "hex", false);
  const scale = window.innerWidth / session.width;
  // Beside the pointer, flipped to the other side near the display's edges.
  const cx = (point?.x ?? 0) * scale;
  const cy = (point?.y ?? 0) * scale;
  const boxHeight = LOUPE_SIZE + 34;
  const left = cx + 24 + LOUPE_SIZE > window.innerWidth ? cx - 24 - LOUPE_SIZE : cx + 24;
  const top = cy + 24 + boxHeight > window.innerHeight ? cy - 24 - boxHeight : cy + 24;
  const dpr = window.devicePixelRatio || 1;

  return (
    <div
      className="cp-surface is-color"
      onPointerMove={(e) => setPoint(toPicture(session, e.clientX, e.clientY))}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        commit(session, { kind: "point", ...toPicture(session, e.clientX, e.clientY) });
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        commit(session, null);
      }}
    >
      <canvas ref={canvas} className="cp-canvas" width={session.width} height={session.height} />
      {point ? (
        <div className="cp-loupe" style={{ left, top }}>
          <canvas
            ref={loupe}
            className="cp-loupe-pixels"
            width={Math.round(LOUPE_SIZE * dpr)}
            height={Math.round(LOUPE_SIZE * dpr)}
            style={{ width: LOUPE_SIZE, height: LOUPE_SIZE }}
          />
          <div className="cp-readout">
            <span className="cp-swatch" style={{ background: hex }} />
            <span className="cp-value">{value}</span>
          </div>
        </div>
      ) : (
        <div className="cp-hint" role="status">
          {S.hintPick} · {S.hintNudge} · {S.hintCancel}
        </div>
      )}
    </div>
  );
}
