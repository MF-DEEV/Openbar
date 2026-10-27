// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { resolve } from "../shared/prefs";
import { demoSnapshot } from "./demo";
import { detailRows, processValue, summary } from "./detail";
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
    const bad = resolve({ island: { monitorInterval: 3, networkSpeedUnit: "nibbles" as never } });
    expect(bad.island.monitorInterval).toBe(2);
    expect(bad.island.networkSpeedUnit).toBe("bytes");
  });
});
