// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../i18n/index";

// The network detail's speed test (MetricDetailView.speedTestCard): a button
// that runs it, a spinner while it does, then download and upload in Mbps
// with the latency under them, or that it failed.

import { Gauge, LoaderCircle } from "lucide-react";
import { COLORS } from "./detail";
import { mbps } from "./format";
import { speedTestRunning, useSystem } from "./store";
import { S } from "./strings";

export function SpeedTestCard() {
  useLocale();
  const speed = useSystem((s) => s.speedTest);
  const start = useSystem((s) => s.startSpeedTest);
  const running = speedTestRunning(speed);
  return (
    <section className="sy-panel sy-speed">
      <div className="sy-speed-head">
        {running ? (
          <span className="sy-speed-testing" role="status">
            <LoaderCircle className="sy-spin" size={13} aria-hidden />
            {S.speedTestTesting}
          </span>
        ) : (
          <button type="button" className="sy-speed-button" onClick={start}>
            <Gauge size={12} strokeWidth={2.2} aria-hidden />
            {speed.downloadMbps === null ? S.speedTestRun : S.speedTestAgain}
          </button>
        )}
        {speed.downloadMbps !== null && speed.uploadMbps !== null ? (
          <span className="sy-speed-result">
            ↓{mbps(speed.downloadMbps)} ↑{mbps(speed.uploadMbps)} {t("windows.system.mbps")}
          </span>
        ) : null}
      </div>
      {speed.phase === "failed" ? (
        <div className="sy-speed-note" style={{ color: COLORS.orange }}>
          {S.speedTestFailed}
        </div>
      ) : speed.latencyMs !== null ? (
        <div className="sy-speed-note">
          {S.speedTestLatency}: {Math.round(speed.latencyMs)} {t("windows.system.ms")}
        </div>
      ) : null}
    </section>
  );
}
