// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Layout (NotchSettings.swift:225-301): size, shape, capsule fit
// and outline. Every Windows display is one without a notch, so the shape
// and fit always apply; the notched-Mac cards have no place here.

import { useLocale } from "../i18n";

import { useLayoutEffect, useRef, useState } from "react";
import { Maximize2, Shapes, Square, SquareDashed } from "lucide-react";
import {
  makeGeometry,
  restingSize,
  edgePath,
  outlineGeometry,
  notchPath,
  notchMetrics,
  type LayoutPreset,
  type Silhouette,
} from "../island/geometry";
import { useSettings } from "./state";
import {
  constrainIslandPosition,
  insetUsableEdge,
  islandEdge,
  placeIsland,
  usableDisplayArea,
} from "../island/placement";
import type { IslandEdge, IslandPosition } from "../island/placement";
import { T } from "./strings";
import { Button, Expander, Group, Page, Row, Segmented, Slider, SubRow, Toggle } from "./ui";

const signed = (v: number) => (v > 0 ? `+${v}` : `${v}`);

function PositionPreview({
  island,
  set,
}: {
  island: ReturnType<typeof useSettings.getState>["prefs"]["island"];
  set: (key: "position", value: IslandPosition) => void;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const lastSave = useRef(0);
  const dragStart = useRef<{ x: number; y: number; position: IslandPosition; width: number; height: number } | null>(
    null,
  );
  const dragEdge = useRef<IslandEdge | null>(null);
  const [dragged, setDragged] = useState<IslandPosition | null>(null);
  const screen = { width: window.screen.width || 1920, height: window.screen.height || 1080 };
  const usable = usableDisplayArea(screen);
  const position = constrainIslandPosition(dragged ?? island.position, screen, usable);
  const edge = islandEdge(position, screen, usable);
  const side = edge === "left" || edge === "right";
  // The same shapes as the island: a notch stays flat against any edge, a capsule floats off it.
  const notch = island.silhouette === "notch";
  const geometry = makeGeometry({
    screen,
    devicePixelRatio: window.devicePixelRatio || 1,
    silhouette: edge === "top" ? island.silhouette : "capsule",
    layout: island.layout,
    notchWidth: island.notchWidth,
    notchHeight: island.notchHeight,
    roundness: island.roundness,
    shoulderWidth: island.shoulderWidth,
    shoulderHeight: island.shoulderHeight,
    capsuleFit: edge !== "top" && notch ? { ...island.capsuleFit, drop: 0 } : island.capsuleFit,
  });
  const outline = edge === "top" ? geometry : outlineGeometry(geometry, island.silhouette);
  const horizontalSize = restingSize(geometry, island.idleContent !== "none");
  const size = side ? { width: horizontalSize.height, height: horizontalSize.width } : horizontalSize;
  const previousPreview = useRef({ edge, size });
  const smoothPosition =
    previousPreview.current.edge === edge &&
    previousPreview.current.size.width === size.width &&
    previousPreview.current.size.height === size.height;
  useLayoutEffect(() => {
    previousPreview.current = { edge, size };
  });
  const placed = placeIsland(position, screen, size, insetUsableEdge(usable, edge, geometry.floatingDrop, screen));
  const update = (clientX: number, clientY: number, save: boolean) => {
    const bounds = surface.current?.getBoundingClientRect();
    const start = dragStart.current;
    if (!bounds || !start) return;
    const next = constrainIslandPosition(
      {
        x:
          start.position.x +
          ((clientX - start.x) * screen.width) / bounds.width / Math.max(1, usable.right - usable.left - start.width),
        y:
          start.position.y +
          ((clientY - start.y) * screen.height) /
            bounds.height /
            Math.max(1, usable.bottom - usable.top - start.height),
      },
      screen,
      usable,
      dragEdge.current ?? undefined,
    );
    dragEdge.current = islandEdge(next, screen, usable);
    setDragged(next);
    if (save) set("position", next);
  };

  return (
    <div className="st-island-position">
      <div
        className="st-island-monitor"
        ref={surface}
        style={{
          maxWidth: `${(360 * screen.width) / screen.height}px`,
          aspectRatio: `${screen.width} / ${screen.height}`,
        }}
        onPointerMove={(event) => {
          if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
          const save = performance.now() - lastSave.current > 50;
          update(event.clientX, event.clientY, save);
          if (save) lastSave.current = performance.now();
        }}
        onPointerUp={(event) => {
          if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
          update(event.clientX, event.clientY, true);
          event.currentTarget.releasePointerCapture(event.pointerId);
          dragStart.current = null;
          dragEdge.current = null;
          setDragged(null);
        }}
        onPointerCancel={() => {
          dragStart.current = null;
          dragEdge.current = null;
          setDragged(null);
        }}
      >
        <svg
          className="st-island-wallpaper"
          viewBox="0 0 800 450"
          preserveAspectRatio="xMidYMid slice"
          aria-hidden="true"
        >
          <defs>
            <radialGradient id="st-island-blue">
              <stop stopColor="#273ebd" />
              <stop offset="0.55" stopColor="#111c57" />
              <stop offset="1" stopColor="#080c20" />
            </radialGradient>
          </defs>
          <rect width="800" height="450" fill="url(#st-island-blue)" />
          <path d="M0 450 C150 320 260 500 420 320 S700 320 800 190 L800 450Z" fill="#1727a9" opacity=".8" />
          <path
            d="M80 450 C200 250 330 470 500 280 S750 290 800 150"
            fill="none"
            stroke="#2d45ea"
            strokeWidth="55"
            opacity=".55"
          />
        </svg>
        <button
          type="button"
          className={`st-island-drag${smoothPosition ? " is-position-moving" : ""}`}
          aria-label={T.layout.dragIsland}
          style={{
            left: `${(100 * placed.left) / screen.width}%`,
            top: `${(100 * placed.top) / screen.height}%`,
            width: `${(100 * size.width) / screen.width}%`,
            height: `${(100 * size.height) / screen.height}%`,
          }}
          onPointerDown={(event) => {
            event.preventDefault();
            dragStart.current = {
              x: event.clientX,
              y: event.clientY,
              position,
              width: size.width,
              height: size.height,
            };
            dragEdge.current = edge;
            surface.current?.setPointerCapture(event.pointerId);
          }}
        >
          <svg viewBox={`0 0 ${size.width} ${size.height}`} preserveAspectRatio="none" aria-hidden="true">
            <path
              d={edgePath(outline, size, edge)}
              fill="#050507"
              stroke={island.outline ? "#aaa" : "#3436c7"}
              strokeWidth={island.outline ? 2 : 1}
            />
          </svg>
        </button>
      </div>
      <div className="st-island-position-footer">
        <span>{T.layout.positionHint}</span>
        <Button onClick={() => set("position", { x: 0.5, y: 0 })}>{T.layout.restorePosition}</Button>
      </div>
    </div>
  );
}

export function LayoutSettings() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const custom = island.layout === "custom";
  const fit = island.capsuleFit;
  const screen = { width: window.screen.width || 1920, height: window.screen.height || 1080 };
  const usable = usableDisplayArea(screen);
  const edge = islandEdge(constrainIslandPosition(island.position, screen, usable), screen, usable);
  const side = edge === "left" || edge === "right";
  const distanceLabel = {
    top: T.layout.fromTop,
    bottom: T.layout.fromBottom,
    left: T.layout.fromLeft,
    right: T.layout.fromRight,
  }[edge];
  const ears = notchMetrics(island.notchWidth, island.notchHeight, island);
  const capsuleGeometry = makeGeometry({
    screen: { width: 1920, height: 1080 },
    devicePixelRatio: window.devicePixelRatio || 1,
    silhouette: "capsule",
    layout: island.layout,
    capsuleFit: fit,
    roundness: island.roundness,
  });
  const capsuleSize = restingSize(capsuleGeometry);
  const previewSize = side ? { width: capsuleSize.height, height: capsuleSize.width } : capsuleSize;
  const previewX =
    edge === "left"
      ? 8 + fit.drop
      : edge === "right"
        ? 192 - fit.drop - previewSize.width
        : (200 - previewSize.width) / 2;
  const previewY =
    edge === "top"
      ? 8 + fit.drop
      : edge === "bottom"
        ? 56 - fit.drop - previewSize.height
        : (96 - previewSize.height) / 2;

  return (
    <Page title={T.nav.layout}>
      <Group>
        <PositionPreview island={island} set={set} />
        <Row icon={Shapes} title={T.layout.shape} caption={T.layout.shapeCaption}>
          <Segmented<Silhouette>
            label={T.layout.shape}
            value={island.silhouette}
            onChange={(v) => set("silhouette", v)}
            options={[
              { value: "notch", label: T.layout.notch },
              { value: "capsule", label: T.layout.capsule },
            ]}
          />
        </Row>

        {island.silhouette === "notch" ? (
          <Expander icon={SquareDashed} title={T.layout.notchFit} caption={T.layout.notchFitCaption} defaultOpen>
            <div className="st-notch-preview">
              <svg
                viewBox={`-12 -8 ${island.notchWidth + 24} ${island.notchHeight + 20}`}
                role="img"
                aria-label={T.layout.notchPreview}
              >
                <path d={notchPath(island.notchWidth, island.notchHeight, island)} fill="#000" />
                <path
                  d={`M0 0 C${ears.s * 0.55228475} 0 ${ears.s} ${ears.rise * (1 - 0.55228475)} ${ears.s} ${ears.rise}`}
                />
                <path
                  d={`M${island.notchWidth} 0 C${island.notchWidth - ears.s * 0.55228475} 0 ${island.notchWidth - ears.s} ${ears.rise * (1 - 0.55228475)} ${island.notchWidth - ears.s} ${ears.rise}`}
                />
              </svg>
              <span>{T.layout.notchPreviewCaption}</span>
            </div>
            <SubRow title={T.layout.shoulderWidth} caption={T.layout.shoulderWidthCaption}>
              <Slider
                label={T.layout.shoulderWidth}
                min={0}
                max={300}
                step={5}
                value={island.shoulderWidth}
                format={(v) => v + "%"}
                onChange={(v) => set("shoulderWidth", v)}
              />
            </SubRow>
            <SubRow title={T.layout.shoulderHeight} caption={T.layout.shoulderHeightCaption}>
              <Slider
                label={T.layout.shoulderHeight}
                min={0}
                max={300}
                step={5}
                value={island.shoulderHeight}
                format={(v) => v + "%"}
                onChange={(v) => set("shoulderHeight", v)}
              />
            </SubRow>
            <SubRow title={T.layout.width}>
              <Slider
                label={T.layout.notchFit + ", " + T.layout.width}
                min={100}
                max={400}
                step={5}
                value={island.notchWidth}
                format={(v) => v + " px"}
                onChange={(v) => set("notchWidth", v)}
              />
            </SubRow>
            <SubRow title={T.layout.height}>
              <Slider
                label={T.layout.notchFit + ", " + T.layout.height}
                min={20}
                max={64}
                step={1}
                value={island.notchHeight}
                format={(v) => v + " px"}
                onChange={(v) => set("notchHeight", v)}
              />
            </SubRow>
          </Expander>
        ) : null}

        {island.silhouette === "capsule" ? (
          <Expander
            icon={SquareDashed}
            title={T.layout.capsuleFit}
            caption={
              edge === "top"
                ? T.layout.capsuleFitCaption
                : edge === "bottom"
                  ? T.layout.capsuleFitCaptionBottom
                  : T.layout.capsuleFitCaptionSide
            }
            defaultOpen
          >
            <div className="st-notch-preview">
              <svg viewBox={`0 0 200 ${side ? 96 : 64}`} role="img" aria-label={T.layout.capsulePreview}>
                {side ? (
                  <line
                    x1={edge === "left" ? 8 : 192}
                    x2={edge === "left" ? 8 : 192}
                    y1="0"
                    y2="96"
                    stroke="#777"
                    strokeWidth="0.5"
                  />
                ) : (
                  <line
                    x1="0"
                    x2="200"
                    y1={edge === "top" ? 8 : 56}
                    y2={edge === "top" ? 8 : 56}
                    stroke="#777"
                    strokeWidth="0.5"
                  />
                )}
                <g transform={`translate(${previewX} ${previewY})`}>
                  <path d={edgePath(capsuleGeometry, previewSize, edge)} fill="#000" />
                </g>
              </svg>
              <span>{edge === "top" ? T.layout.capsulePreviewCaption : T.layout.capsulePreviewCaptionEdge}</span>
            </div>
            <SubRow title={T.layout.width}>
              <Slider
                label={`${T.layout.capsuleFit}, ${T.layout.width}`}
                min={-40}
                max={40}
                step={2}
                value={fit.width}
                format={signed}
                onChange={(v) => set("capsuleFit", { ...fit, width: v })}
              />
            </SubRow>
            <SubRow title={T.layout.height}>
              <Slider
                label={`${T.layout.capsuleFit}, ${T.layout.height}`}
                min={-4}
                max={4}
                step={1}
                value={fit.height}
                format={signed}
                onChange={(v) => set("capsuleFit", { ...fit, height: v })}
              />
            </SubRow>
            <SubRow title={distanceLabel}>
              <Slider
                label={`${T.layout.capsuleFit}, ${distanceLabel}`}
                min={0}
                max={20}
                step={1}
                value={fit.drop}
                onChange={(v) => set("capsuleFit", { ...fit, drop: v })}
              />
            </SubRow>
          </Expander>
        ) : null}

        <Row icon={Shapes} title={T.layout.corners} caption={T.layout.cornersCaption}>
          <Slider
            label={T.layout.corners}
            min={0}
            max={200}
            step={5}
            value={island.roundness}
            format={(v) => v + "%"}
            onChange={(v) => set("roundness", v)}
          />
        </Row>

        <Row title={T.layout.resetShape}>
          <Button
            onClick={() =>
              useSettings.getState().patch({
                island: {
                  notchWidth: null,
                  notchHeight: null,
                  roundness: null,
                  shoulderWidth: null,
                  shoulderHeight: null,
                  capsuleFit: null,
                },
              })
            }
          >
            {T.layout.resetShape}
          </Button>
        </Row>

        <Expander
          icon={Maximize2}
          title={T.layout.size}
          caption={T.layout.sizeCaption}
          defaultOpen={false}
          control={
            <Segmented<LayoutPreset>
              label={T.layout.size}
              value={island.layout}
              onChange={(v) => set("layout", v)}
              options={[
                { value: "compact", label: T.layout.compact },
                { value: "spacious", label: T.layout.spacious },
                { value: "custom", label: T.layout.custom },
              ]}
            />
          }
        >
          <SubRow title={T.layout.width} disabled={!custom}>
            <Slider
              label={T.layout.width}
              min={360}
              max={600}
              step={10}
              value={island.customWidth}
              disabled={!custom}
              format={(v) => `${v} px`}
              onChange={(v) => set("customWidth", v)}
            />
          </SubRow>
          <SubRow title={T.layout.maxHeight} caption={T.layout.customCaption} disabled={!custom}>
            <Slider
              label={T.layout.maxHeight}
              min={260}
              max={640}
              step={10}
              value={island.customHeight}
              disabled={!custom}
              format={(v) => `${v} px`}
              onChange={(v) => set("customHeight", v)}
            />
          </SubRow>
        </Expander>

        <Row icon={Square} title={T.layout.outline} caption={T.layout.outlineCaption}>
          <Toggle label={T.layout.outline} checked={island.outline} onChange={(on) => set("outline", on)} />
        </Row>
      </Group>
    </Page>
  );
}
