// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The System section's strings: the Mac's English (Localization.swift,
// ConnectedDevicesStrings.swift, BatteryTimeStrings.swift), with "Mac" read
// as "PC".

import { t, tf } from "../i18n";

export const S = {
  get cpu() {
    return t("s.monitorShowCPU");
  },
  get gpu() {
    return t("s.monitorShowGPU");
  },
  get memory() {
    return t("s.monitorShowMemory");
  },
  get memoryDetails() {
    return t("windows.system.memoryDetails");
  },
  get memoryPressure() {
    return t("windows.system.memoryPressure");
  },
  get memoryCompressed() {
    return t("windows.system.memoryCompressed");
  },
  get memoryCached() {
    return t("windows.system.memoryCached");
  },
  get memorySwapUsed() {
    return t("windows.system.memorySwapUsed");
  },
  get metricUnavailable() {
    return t("notchShape.unavailable");
  },
  get memoryPressureHelp() {
    return t("windows.system.memoryPressureHelp");
  },
  get memoryCompressedHelp() {
    return t("windows.system.memoryCompressedHelp");
  },
  get memoryCachedHelp() {
    return t("windows.system.memoryCachedHelp");
  },
  get memorySwapUsedHelp() {
    return t("windows.system.memorySwapUsedHelp");
  },
  get battery() {
    return t("commandBar.answerBatteryLabel");
  },
  get network() {
    return t("s.monitorShowNetwork");
  },
  /** The disk card (diskAvailable, capitalized). */
  get available() {
    return t("s.diskAvailable");
  },
  get disks() {
    return t("s.diskSection");
  },
  get power() {
    return t("s.monitorShowPowerLabel");
  },
  get connectedDevices() {
    return t("connectedDevices.title");
  },
  get unavailable() {
    return t("windows.system.unavailable");
  },
  get back() {
    return t("radialMenu.backButton");
  },
  get usage() {
    return t("s.usageSection");
  },
  get networkApps() {
    return t("windows.system.networkApps");
  },
  get energyApps() {
    return t("windows.system.energyApps");
  },
  get processesIdle() {
    return t("windows.system.processesIdle");
  },
  get networkAppsIdle() {
    return t("windows.system.networkAppsIdle");
  },
  get processesUnavailable() {
    return t("windows.system.processesUnavailable");
  },
  get networkPermissionDenied() {
    return t("windows.system.networkPermissionDenied");
  },
  get networkNeedsApproval() {
    return t("windows.system.networkNeedsApproval");
  },
  get networkApprove() {
    return t("notchClaudeSessions.allow");
  },
  get networkApproving() {
    return t("windows.system.networkApproving");
  },
  get networkDeclined() {
    return t("windows.system.networkDeclined");
  },
  get networkIncomplete() {
    return t("windows.system.networkIncomplete");
  },
  get energyUnavailable() {
    return t("windows.system.energyUnavailable");
  },
  /** monitorOpenActivityMonitor, Task Manager being the PC's Activity Monitor. */
  get openTaskManager() {
    return t("windows.system.openTaskManager");
  },
  get upFor() {
    return t("s.systemUptime");
  },
  get measuring() {
    return t("s.breakdownMeasuring");
  },
  get download() {
    return t("s.networkDownload");
  },
  get upload() {
    return t("s.networkUpload");
  },
  get thisSession() {
    return t("s.networkThisSession");
  },
  get used() {
    return t("s.diskUsed");
  },
  get availableRow() {
    return t("s.diskAvailable");
  },
  get read() {
    return t("s.diskRead");
  },
  get write() {
    return t("s.diskWrite");
  },
  get charge() {
    return t("s.batteryCharge");
  },
  get batteryHealth() {
    return t("s.powerHealth");
  },
  get cycles() {
    return t("s.powerCycles");
  },
  get speedTestRun() {
    return t("s.monitorItemNetTest");
  },
  get speedTestAgain() {
    return t("s.speedTestAgain");
  },
  get speedTestLatency() {
    return t("s.speedTestLatency");
  },
  get speedTestTesting() {
    return t("s.speedTestTesting");
  },
  get speedTestFailed() {
    return t("s.speedTestFailed");
  },
  get system() {
    return t("appearance.system");
  },
  get adapter() {
    return t("s.powerAdapter");
  },
  get charging() {
    return t("notch.charging");
  },
  get onBattery() {
    return t("notch.onBattery");
  },
  get pluggedIn() {
    return t("s.powerPluggedIn");
  },
  get powerUnavailable() {
    return t("windows.system.powerUnavailable");
  },
  get batteryTime() {
    return t("batteryTime.title");
  },
  get calculating() {
    return t("batteryTime.calculating");
  },
  get noDisks() {
    return t("s.diskNoDisks");
  },
  get noDevices() {
    return t("connectedDevices.noDevices");
  },
  get unnamedUsb() {
    return t("connectedDevices.unnamedDevice");
  },
  get unnamedBluetooth() {
    return t("windows.system.unnamedBluetooth");
  },
  devicesConnected: (count: number) =>
    count === 1 ? t("connectedDevices.oneConnected") : tf("connectedDevices.devicesConnectedFormat", count),
} as const;
