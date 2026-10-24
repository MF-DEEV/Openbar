// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Layout (NotchSettings.swift:225-301): size, shape, capsule fit
// and outline. Every Windows display is one without a notch, so the shape
// and fit always apply; the notched-Mac cards have no place here.

import { Maximize2, Shapes, Square, SquareDashed } from "lucide-react";
import type { LayoutPreset, Silhouette } from "../island/geometry";
import { useSettings } from "./state";
import { T } from "./strings";
import { Expander, Group, Page, Row, Segmented, Slider, SubRow, Toggle } from "./ui";

const signed = (v: number) => (v > 0 ? `+${v}` : `${v}`);

export function LayoutSettings() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const custom = island.layout === "custom";
  const fit = island.capsuleFit;

  return (
    <Page title={T.nav.layout}>
      <Group>
        <Expander
          icon={Maximize2}
          title={T.layout.size}
          caption={T.layout.sizeCaption}
          defaultOpen={custom}
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

        {island.silhouette === "capsule" ? (
          <Expander icon={SquareDashed} title={T.layout.capsuleFit} caption={T.layout.capsuleFitCaption}>
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

        <Row icon={Square} title={T.layout.outline} caption={T.layout.outlineCaption}>
          <Toggle label={T.layout.outline} checked={island.outline} onChange={(on) => set("outline", on)} />
        </Row>
      </Group>
    </Page>
  );
}
