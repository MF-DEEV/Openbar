// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The resting charge can turn red once it runs low, and amber a little
// earlier, at levels the person picks (NotchSettings.swift:lowBatteryControls).

import { BatteryLow, BatteryMedium, PanelBottom } from "lucide-react";
import { BATTERY_LEVEL_SCALE, earlyBatteryThreshold, lowBatteryThreshold } from "../island/lowBattery";
import { useSettings } from "./state";
import { T } from "./strings";
import { Row, Slider, Toggle } from "./ui";

const percent = (value: number) => `${value}%`;

export function LowBatteryControls() {
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const S = T.content.lowBattery;
  const red = lowBatteryThreshold(island.lowBatteryThreshold);
  return (
    <>
      <Row icon={BatteryLow} title={S.title} caption={S.caption}>
        <Toggle
          label={S.title}
          checked={island.lowBatteryTint}
          onChange={(lowBatteryTint) => patch({ island: { lowBatteryTint } })}
        />
      </Row>
      {island.lowBatteryTint ? (
        <>
          <Row title={S.threshold}>
            <Slider
              {...BATTERY_LEVEL_SCALE}
              step={1}
              label={S.threshold}
              format={percent}
              value={red}
              onChange={(value) => {
                // Raising red past amber carries amber along above it.
                const next = lowBatteryThreshold(value);
                patch({
                  island: {
                    lowBatteryThreshold: next,
                    lowBatteryEarlyThreshold: earlyBatteryThreshold(island.lowBatteryEarlyThreshold, next),
                  },
                });
              }}
            />
          </Row>
          <Row icon={BatteryMedium} title={S.early} caption={S.earlyCaption}>
            <Toggle
              label={S.early}
              checked={island.lowBatteryEarly}
              onChange={(lowBatteryEarly) => patch({ island: { lowBatteryEarly } })}
            />
          </Row>
          {island.lowBatteryEarly ? (
            <Row title={S.earlyThreshold}>
              <Slider
                {...BATTERY_LEVEL_SCALE}
                step={1}
                label={S.earlyThreshold}
                format={percent}
                value={earlyBatteryThreshold(island.lowBatteryEarlyThreshold, red)}
                onChange={(value) => patch({ island: { lowBatteryEarlyThreshold: earlyBatteryThreshold(value, red) } })}
              />
            </Row>
          ) : null}
          <Row icon={PanelBottom} title={S.tray}>
            <Toggle
              label={S.tray}
              checked={island.lowBatteryTray}
              onChange={(lowBatteryTray) => patch({ island: { lowBatteryTray } })}
            />
          </Row>
        </>
      ) : null}
    </>
  );
}
