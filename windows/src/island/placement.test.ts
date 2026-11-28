// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { constrainIslandPosition, insetUsableEdge, islandEdge, placeIsland, surfaceOffset } from "./placement";
import { alongInset, capsuleBody, edgePath, islandPath, makeGeometry, outlineGeometry } from "./geometry";

describe("free island placement", () => {
  const screen = { width: 1920, height: 1080 };

  it("preserves the default top centre", () => {
    expect(placeIsland({ x: 0.5, y: 0 }, screen, { width: 200, height: 30 })).toEqual({ left: 860, top: 0 });
  });

  it("applies the capsule distance inward from the selected work-area edge", () => {
    const area = { left: 48, right: 1872, top: 40, bottom: 1040 };
    const size = { width: 200, height: 30 };
    for (const [edge, position, coordinate, expected] of [
      ["top", { x: 0.5, y: 0 }, "top", 55],
      ["bottom", { x: 0.5, y: 1 }, "top", 995],
      ["left", { x: 0, y: 0.5 }, "left", 63],
      ["right", { x: 1, y: 0.5 }, "left", 1657],
    ] as const) {
      const inset = insetUsableEdge(area, edge, 15, screen);
      expect(placeIsland(position, screen, size, inset)[coordinate]).toBe(expected);
    }
  });

  it("uses the full usable travel along each edge without a dead zone", () => {
    const size = { width: 200, height: 30 };
    const area = { left: 48, right: 1872, top: 40, bottom: 1040 };
    const start = placeIsland({ x: 0, y: 0 }, screen, size, area);
    const end = placeIsland({ x: 1, y: 0 }, screen, size, area);
    expect(start.left).toBeGreaterThanOrEqual(48);
    expect(start.left).toBeLessThan(49);
    expect(end.left).toBeGreaterThan(1671);
    expect(end.left).toBeLessThanOrEqual(1672);
    expect(placeIsland({ x: 0.99, y: 0 }, screen, size, area).left).toBeLessThan(end.left);
    expect(placeIsland({ x: 0, y: 1 }, screen, { width: 48, height: 128 }, area).top).toBe(912);
  });

  it("projects old centre positions onto the nearest physical edge", () => {
    const area = { left: 80, right: 1840, top: 40, bottom: 1040 };
    const projected = constrainIslandPosition({ x: 0.4, y: 0.3 }, screen, area);
    expect(projected).toEqual({ x: 0.4, y: 0 });
    expect(constrainIslandPosition({ x: 0.96, y: 0.5 }, screen, area)).toEqual({ x: 1, y: 0.5 });
    expect(constrainIslandPosition({ x: 0.5, y: 0.93 }, screen, area)).toEqual({ x: 0.5, y: 1 });
  });

  it("keeps corners on their chosen edge and damps edge changes during drag", () => {
    const area = { top: 0, bottom: 1080 };
    const top = constrainIslandPosition({ x: 0, y: 0 }, screen, area);
    expect(islandEdge(top, screen, area)).toBe("top");
    const left = constrainIslandPosition({ x: 0.002, y: 0.006 }, screen, area, "left");
    expect(islandEdge(left, screen, area)).toBe("left");
    expect(constrainIslandPosition({ x: 0.03, y: 0.002 }, screen, area, "left").y).toBe(0);
  });

  it("projects invalid saved coordinates into the work area", () => {
    const projected = constrainIslandPosition({ x: -0.5, y: 4 }, screen, { top: 40, bottom: 1040 });
    expect(projected.x).toBeGreaterThanOrEqual(0);
    expect(projected.y).toBeLessThanOrEqual(1);
    const frame = placeIsland(projected, screen, { width: 200, height: 30 }, { top: 40, bottom: 1040 });
    expect(frame.left).toBeGreaterThanOrEqual(0);
    expect(frame.top).toBeGreaterThanOrEqual(40);
    expect(frame.top + 30).toBeLessThanOrEqual(1040);
  });

  it("keeps every presentation and its quick buttons inside the work area", () => {
    const area = { top: 40, bottom: 1040 };
    for (const size of [
      { width: 200, height: 30 },
      { width: 560, height: 530 },
      { width: 420, height: 270 },
    ]) {
      for (const position of [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
        { x: 0.12, y: 0.73 },
      ]) {
        const { left, top } = placeIsland(position, screen, size, area, true);
        const edge = islandEdge(constrainIslandPosition(position, screen, area), screen, area);
        const side = edge === "left" || edge === "right";
        expect(left).toBeGreaterThanOrEqual(side ? 0 : 56);
        expect(left + size.width + (side ? 0 : 56)).toBeLessThanOrEqual(screen.width);
        expect(top).toBeGreaterThanOrEqual(area.top);
        expect(top + size.height + (edge === "bottom" || side ? 0 : 56)).toBeLessThanOrEqual(area.bottom);
      }
    }
  });

  it("hangs an open bottom panel from the taskbar like the closed island", () => {
    const area = { top: 40, bottom: 1040 };
    const size = { width: 450, height: 300 };
    const open = placeIsland({ x: 0.5, y: 1 }, screen, size, area, true, 30, 200);
    const closed = placeIsland({ x: 0.5, y: 1 }, screen, { width: 200, height: 30 }, area);
    expect(open.top + size.height).toBe(area.bottom);
    expect(closed.top + 30).toBe(area.bottom);
  });

  it("opens above a resting island in the lower half", () => {
    expect(
      placeIsland({ x: 0.5, y: 0.8 }, screen, { width: 480, height: 400 }, { top: 0, bottom: 1040 }, false, 30).top,
    ).toBe(640);
  });

  it("stays clear of a taskbar on the left or right", () => {
    const area = { left: 48, right: 1872, top: 0, bottom: 1080 };
    expect(placeIsland({ x: 0, y: 0.5 }, screen, { width: 200, height: 30 }, area).left).toBe(48);
    expect(placeIsland({ x: 1, y: 0.5 }, screen, { width: 200, height: 30 }, area).left).toBe(1672);
  });

  it("uses the nearest physical edge at each corner", () => {
    const area = { left: 0, right: 1920, top: 0, bottom: 1080 };
    expect(islandEdge({ x: 0.05, y: 0.05 }, screen, area)).toBe("top");
    expect(islandEdge({ x: 0.02, y: 0.2 }, screen, area)).toBe("left");
    expect(islandEdge({ x: 0.98, y: 0.8 }, screen, area)).toBe("right");
    expect(islandEdge({ x: 0.05, y: 0.98 }, screen, area)).toBe("bottom");
  });

  it("anchors the animated contour to its opening edge", () => {
    const target = { width: 440, height: 520 };
    const drawn = { width: 100, height: 30 };
    expect(surfaceOffset(target, drawn, "top")).toEqual({ x: 170, y: 0 });
    expect(surfaceOffset(target, drawn, "bottom")).toEqual({ x: 170, y: 490 });
    expect(surfaceOffset(target, drawn, "left")).toEqual({ x: 0, y: 245 });
    expect(surfaceOffset(target, drawn, "right")).toEqual({ x: 340, y: 245 });
    expect(surfaceOffset(target, drawn, "top", { x: 0, y: 0 })).toEqual({ x: 0, y: 0 });
    expect(surfaceOffset(target, drawn, "left", { x: 0, y: 0 })).toEqual({ x: 0, y: 0 });
    expect(surfaceOffset(target, drawn, "bottom", { x: 1, y: 1 })).toEqual({ x: 340, y: 490 });
  });

  it("expands inward from a side while retaining the compact island's vertical centre", () => {
    const area = { left: 0, right: 1920, top: 0, bottom: 1080 };
    const size = { width: 440, height: 520 };
    expect(placeIsland({ x: 0.05, y: 0.5 }, screen, size, area, false, 128, 48)).toEqual({ left: 0, top: 280 });
    expect(placeIsland({ x: 0.95, y: 0.5 }, screen, size, area, false, 128, 48)).toEqual({ left: 1480, top: 280 });
  });

  it("keeps a usable body for the vertical capsule", () => {
    expect(capsuleBody(48, 128, 2).width).toBe(36);
  });
});

describe("the outline on each edge", () => {
  const screen = { width: 1920, height: 1080 };
  const notch = outlineGeometry(
    makeGeometry({ screen, devicePixelRatio: 1, silhouette: "capsule", layout: "spacious" }),
    "notch",
  );
  const points = (d: string) =>
    [...d.matchAll(/(-?\d*\.?\d+(?:e-?\d+)?) (-?\d*\.?\d+(?:e-?\d+)?)/g)].map((m) => [Number(m[1]), Number(m[2])]);
  const bounds = (d: string) => {
    const p = points(d);
    return {
      left: Math.min(...p.map(([x]) => x)),
      right: Math.max(...p.map(([x]) => x)),
      top: Math.min(...p.map(([, y]) => y)),
      bottom: Math.max(...p.map(([, y]) => y)),
    };
  };

  it("keeps a notch flat against whichever edge it hangs from", () => {
    const size = { width: 400, height: 300 };
    expect(edgePath(notch, size, "top")).toBe(islandPath(notch, size));
    for (const edge of ["top", "bottom", "left", "right"] as const) {
      expect(bounds(edgePath(notch, size, edge))).toEqual({ left: 0, right: 400, top: 0, bottom: 300 });
      // The flat side runs the full length of the edge: both of its corners are on it.
      const corners = points(edgePath(notch, size, edge)).filter(([x, y]) =>
        edge === "top" ? y === 0 : edge === "bottom" ? y === 300 : edge === "left" ? x === 0 : x === 400,
      );
      expect(corners.length).toBeGreaterThanOrEqual(2);
    }
  });

  it("insets content along a side edge by the ears", () => {
    expect(alongInset(notch, { width: 400, height: 300 }, "left")).toBe(14);
    expect(alongInset(notch, { width: 400, height: 300 }, "top")).toBe(14);
  });
});
