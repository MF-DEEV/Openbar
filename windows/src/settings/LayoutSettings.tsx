// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Layout (NotchSettings.swift:225-301): size, shape, capsule fit
// and outline. Every Windows display is one without a notch, so the shape
// and fit always apply; the notched-Mac cards have no place here.

import { Maximize2, Shapes, Square, SquareDashed } from "lucide-react";
import {
  makeGeometry,
  restingSize,
  islandPath,
  notchPath,
  notchMetrics,
  type LayoutPreset,
  type Silhouette,
} from "../island/geometry";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Expander, Group, Page, Row, Segmented, Slider, SubRow, Toggle } from "./ui";

const signed = (v: number) => (v > 0 ? `+${v}` : `${v}`);

export function LayoutSettings() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const custom = island.layout === "custom";
  const fit = island.capsuleFit;
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

  return (
    <Page title={T.nav.layout}>
      <Group>
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
          <Expander icon={SquareDashed} title={T.layout.capsuleFit} caption={T.layout.capsuleFitCaption} defaultOpen>
            <div className="st-notch-preview">
              <svg viewBox="0 0 200 64" role="img" aria-label={T.layout.capsulePreview}>
                <line x1="0" x2="200" y1="8" y2="8" stroke="#777" strokeWidth="0.5" />
                <g transform={`translate(${(200 - capsuleSize.width) / 2} ${8 + capsuleGeometry.floatingDrop})`}>
                  <path d={islandPath(capsuleGeometry, capsuleSize)} fill="#000" />
                </g>
              </svg>
              <span>{T.layout.capsulePreviewCaption}</span>
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
            <SubRow title={T.layout.fromTop}>
              <Slider
                label={`${T.layout.capsuleFit}, ${T.layout.fromTop}`}
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
