// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// System options (MonitorSettings.swift readings card, and the monitor
// features the Mac switches in its feature hub): how often the cards read,
// the network unit, and which cards show.

import { useLocale } from "../i18n";

import { MONITOR_METRICS, type MonitorMetric, type NetworkSpeedUnit } from "../system/types";
import { useSettings } from "./state";
import { T } from "./strings";
import { Segmented, SubRow, Toggle } from "./ui";

export function SystemOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const shown = (metric: MonitorMetric) => !island.hiddenSystemMetrics.includes(metric);
  const toggle = (metric: MonitorMetric, on: boolean) => {
    const rest = island.hiddenSystemMetrics.filter((m) => m !== metric);
    set("hiddenSystemMetrics", on ? rest : [...rest, metric]);
  };
  return (
    <>
      <SubRow title={T.system.interval} caption={T.system.intervalCaption}>
        <Segmented<"1" | "2" | "5">
          label={T.system.interval}
          value={String(island.monitorInterval) as "1" | "2" | "5"}
          options={[
            { value: "1", label: T.system.interval1 },
            { value: "2", label: T.system.interval2 },
            { value: "5", label: T.system.interval5 },
          ]}
          onChange={(v) => set("monitorInterval", Number(v))}
        />
      </SubRow>
      <SubRow title={T.system.speedUnit}>
        <Segmented<NetworkSpeedUnit>
          label={T.system.speedUnit}
          value={island.networkSpeedUnit}
          options={[
            { value: "bytes", label: "MB/s" },
            { value: "bits", label: "Mbps" },
          ]}
          onChange={(v) => set("networkSpeedUnit", v)}
        />
      </SubRow>
      {MONITOR_METRICS.map((metric) => (
        <SubRow key={metric} title={T.system.metrics[metric]} caption={T.system.captions[metric]}>
          <Toggle label={T.system.metrics[metric]} checked={shown(metric)} onChange={(on) => toggle(metric, on)} />
        </SubRow>
      ))}
    </>
  );
}
