// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  mbps,
  batteryTime,
  bitsPerSec,
  bitsPerSecCompact,
  bytes,
  bytesPerSec,
  bytesPerSecCompact,
  diskBytes,
  graphCeiling,
  networkGraphCeiling,
  networkRate,
  percent,
  uptime,
  watts,
  wattsCompact,
} from "./format";

describe("system formats", () => {
  it("rounds and clamps percentages", () => {
    expect(percent(0.123)).toBe("12%");
    expect(percent(0.855)).toBe("86%");
    expect(percent(1.4)).toBe("100%");
    expect(percent(-0.2)).toBe("0%");
  });

  it("writes byte rates base 1024 with one decimal under ten", () => {
    expect(bytesPerSec(0)).toBe("0 B/s");
    expect(bytesPerSec(512)).toBe("512 B/s");
    expect(bytesPerSec(1536)).toBe("1.5 KB/s");
    expect(bytesPerSec(2.5 * 1024 ** 2)).toBe("2.5 MB/s");
    expect(bytesPerSec(42 * 1024 ** 2)).toBe("42 MB/s");
    // 1023.6 KB prints as 1024, so it reads as the next unit.
    expect(bytesPerSec(1023.6 * 1024)).toBe("1.0 MB/s");
    expect(bytes(16 * 1024 ** 3)).toBe("16 GB");
    expect(bytes(7.94 * 1024 ** 3)).toBe("7.9 GB");
  });

  it("writes bit rates base 1000 from bytes per second", () => {
    expect(bitsPerSec(0)).toBe("0 bps");
    expect(bitsPerSec(125)).toBe("1.0 Kbps");
    expect(bitsPerSec(1_500_000)).toBe("12 Mbps");
    expect(bitsPerSec(124_950)).toBe("1.0 Mbps");
    expect(networkRate(1_500_000, true)).toBe("12 Mbps");
    expect(networkRate(1_500_000, false)).toBe("1.4 MB/s");
  });

  it("writes compact rates for the detail's summary", () => {
    expect(bytesPerSecCompact(0)).toBe("0B");
    expect(bytesPerSecCompact(1.2 * 1024 ** 2)).toBe("1.2M");
    expect(bytesPerSecCompact(320 * 1024)).toBe("320K");
    expect(bytesPerSecCompact(NaN)).toBe("0B");
    expect(bitsPerSecCompact(1_200_000)).toBe("9.6Mb");
    expect(bitsPerSecCompact(40_000)).toBe("320Kb");
  });

  it("writes disk sizes base 1000, as Explorer labels drives", () => {
    expect(diskBytes(0)).toBe("0 B");
    expect(diskBytes(999)).toBe("999 B");
    expect(diskBytes(41_200_000_000)).toBe("41 GB");
    expect(diskBytes(4_120_000_000)).toBe("4.1 GB");
    expect(diskBytes(1_200_000_000_000)).toBe("1.2 TB");
  });

  it("writes watts, uptime and battery time", () => {
    expect(watts(8.46)).toBe("8.5 W");
    expect(watts(23.4)).toBe("23 W");
    expect(wattsCompact(9.4)).toBe("9W");
    expect(uptime(59)).toBe("0min");
    expect(uptime(2 * 3600 + 5 * 60)).toBe("2h 5min");
    expect(uptime(3 * 86_400 + 4 * 3600)).toBe("3d 4h");
    expect(batteryTime(4_200)).toBe("1h 10m");
    expect(batteryTime(30)).toBe("0h 1m");
    expect(batteryTime(null)).toBeNull();
    expect(batteryTime(0)).toBeNull();
  });

  it("rounds graph ceilings up to 1, 2 or 5 of a unit", () => {
    expect(graphCeiling(0.7, 1024)).toBe(1);
    expect(graphCeiling(3, 1024)).toBe(5);
    expect(graphCeiling(700, 1024)).toBe(1024);
    expect(graphCeiling(1500, 1024)).toBe(2048);
    expect(graphCeiling(12, 1000)).toBe(20);
    expect(networkGraphCeiling(1_500_000, true)).toBe(2_500_000);
    expect(networkGraphCeiling(1_500_000, false)).toBe(2 * 1024 ** 2);
  });
});

describe("speed test results", () => {
  it("drops the decimal from 100 Mbps", () => {
    expect(mbps(0)).toBe("0.0");
    expect(mbps(9.94)).toBe("9.9");
    expect(mbps(99.94)).toBe("99.9");
    expect(mbps(100)).toBe("100");
    expect(mbps(412.6)).toBe("413");
  });
});
