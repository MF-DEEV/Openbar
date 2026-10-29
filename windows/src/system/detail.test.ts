// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { resolve } from "../shared/prefs";
import { demoSnapshot } from "./demo";
import { detailRows, processEmptyText, processRowValue, processValue, summary } from "./detail";
import { S } from "./strings";
import { emptyHistory, pushHistory } from "./store";

const snapshot = demoSnapshot(4);
const history = [1, 2, 3, 4].reduce((h, seq) => pushHistory(h, demoSnapshot(seq)), emptyHistory());

describe("metric details", () => {
  it("summarizes each metric with its graph", () => {
    const cpu = summary("cpu", snapshot, history, false);
    expect(cpu.graph?.lines[0]!.values).toHaveLength(4);
    expect(cpu.graph?.label).toBe("100%");
    const network = summary("network", snapshot, history, true);
    expect(network.graph?.lines).toHaveLength(2);
    expect(network.graph?.label).toMatch(/bps$/);
    expect(summary("memory", snapshot, history, false).secondary).toMatch(/ GB \/ 16 GB$/);
    expect(summary("disk", snapshot, history, false).secondary).toBe("41 GB available");
    expect(summary("battery", snapshot, history, false).secondary).toBe("On battery");
    expect(summary("connectedDevices", snapshot, history, false)).toMatchObject({
      primary: "3",
      secondary: "3 devices connected",
      graph: null,
    });
    expect(summary("cpu", { ...snapshot, cpu: null }, history, false).primary).toBe("-");
  });

  it("lists readings Windows has, measuring until they arrive", () => {
    expect(detailRows("cpu", snapshot, false).map((r) => r.title)).toEqual(["Hardware usage", "Up for"]);
    expect(detailRows("cpu", { ...snapshot, cpu: null }, false)[0]!.value).toBe("Measuring…");
    expect(detailRows("disk", snapshot, false)[0]).toEqual({ title: "C:", value: "92% used" });
    expect(detailRows("disk", { ...snapshot, disk: null }, false)).toEqual([
      { title: "Disks", value: "No mounted disks found." },
    ]);
    const power = detailRows("power", snapshot, false);
    expect(power.map((r) => r.title)).toEqual(["System", "Adapter", "Battery", "Battery time remaining"]);
    expect(power.at(-1)!.value).toBe("1h 10m");
    expect(detailRows("battery", snapshot, false)[1]!.value).toMatch(/ W · On battery$/);
    expect(detailRows("battery", snapshot, false).slice(2)).toEqual([
      { title: "Battery health", value: "88%" },
      { title: "Cycles", value: "412" },
    ]);
    const unread = { ...snapshot, battery: { ...snapshot.battery!, healthPercent: null, cycleCount: null } };
    expect(
      detailRows("battery", unread, false)
        .slice(2)
        .map((r) => r.value),
    ).toEqual(["-", "-"]);
    expect(detailRows("connectedDevices", { ...snapshot, devices: [] }, false)).toEqual([
      { title: "No external devices connected", value: "" },
    ]);
    expect(detailRows("connectedDevices", { ...snapshot, devices: [{ name: "", bluetooth: true }] }, false)[0]).toEqual(
      { title: "Bluetooth Device", value: "", bluetooth: true },
    );
  });

  it("formats process rows", () => {
    expect(processValue("cpu", 12.345)).toBe("12.3%");
    expect(processValue("memory", 1.5 * 1024 ** 3)).toBe("1.5 GB");
    expect(processValue("gpu", 45.678)).toBe("45.7%");
  });

  it("separates stale responses, idle lists, warm-up and unavailable measurements", () => {
    const network = { ...snapshot, processesFor: "network" as const };
    expect(processEmptyText("gpu", network)).toBe(S.measuring);
    expect(processEmptyText("network", { ...network, processStatus: "permissionDenied" })).toBe(
      S.networkPermissionDenied,
    );
    expect(processEmptyText("network", { ...network, processStatus: "needsApproval" })).toBe(S.networkNeedsApproval);
    expect(processEmptyText("network", { ...network, processStatus: "incomplete" })).toBe(S.networkIncomplete);
    expect(processEmptyText("network", { ...network, processStatus: "measuring" })).toBe(S.measuring);
    expect(processEmptyText("network", { ...network, processes: [] })).toBe(S.networkAppsIdle);
    expect(processEmptyText("network", network)).toBeNull();
    expect(processEmptyText("gpu", { ...snapshot, processesFor: "gpu", processStatus: "unavailable" })).toBe(
      S.processesUnavailable,
    );
    expect(processEmptyText("power", { ...snapshot, processesFor: "power", processStatus: "unavailable" })).toBe(
      S.energyUnavailable,
    );
  });

  it("shows both network directions in the selected speed unit", () => {
    const row = { name: "Browser", path: null, value: 1_500_000, netDown: 1_000_000, netUp: 500_000 };
    expect(processRowValue("network", row, true)).toBe("↓8.0Mb ↑4.0Mb");
    expect(processRowValue("network", row, false)).toBe("↓977K ↑488K");
    expect(processRowValue("network", { ...row, netDown: null, netUp: 0 }, false)).toBe("↓- ↑0B");
  });
});

describe("system preferences", () => {
  it("keeps only known intervals, units and metrics", () => {
    const fallback = resolve({});
    expect(fallback.island.monitorInterval).toBe(2);
    expect(fallback.island.networkSpeedUnit).toBe("bytes");
    expect(fallback.island.hiddenSystemMetrics).toEqual([]);
    const stored = resolve({
      island: {
        monitorInterval: 5,
        networkSpeedUnit: "bits",
        hiddenSystemMetrics: ["gpu", "fans", "gpu", "power"] as never,
      },
    });
    expect(stored.island.monitorInterval).toBe(5);
    expect(stored.island.networkSpeedUnit).toBe("bits");
    expect(stored.island.hiddenSystemMetrics).toEqual(["gpu", "power"]);
    const bad = resolve({
      island: { monitorInterval: 3, networkSpeedUnit: "nibbles" as never, hiddenSystemMetrics: "gpu" as never },
    });
    expect(bad.island.monitorInterval).toBe(2);
    expect(bad.island.networkSpeedUnit).toBe("bytes");
    expect(bad.island.hiddenSystemMetrics).toEqual([]);
    expect(resolve({ island: { monitorInterval: "2" as never } }).island.monitorInterval).toBe(2);
    expect(resolve({ island: { monitorInterval: NaN } }).island.monitorInterval).toBe(2);
  });
});
