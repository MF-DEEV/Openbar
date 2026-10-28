// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The System section's strings: the Mac's English (Localization.swift,
// ConnectedDevicesStrings.swift, BatteryTimeStrings.swift), with "Mac" read
// as "PC".

export const S = {
  cpu: "CPU",
  gpu: "GPU",
  memory: "Memory",
  memoryDetails: "Memory details",
  memoryPressure: "Memory pressure (approx.)",
  memoryCompressed: "Compressed memory",
  memoryCached: "Cached memory",
  memorySwapUsed: "Page file usage",
  metricUnavailable: "Unavailable",
  memoryPressureHelp:
    "Windows approximation: the higher of physical RAM usage and commit-limit usage. This is not the macOS memory pressure measurement.",
  memoryCompressedHelp:
    "Resident RAM occupied by the Windows MemCompression store; not the original size of the pages before compression. Unavailable when Windows does not expose the store process.",
  memoryCachedHelp:
    "Windows system cache: standby list plus system working set. This differs from macOS Cached Files and Task Manager's Cached value.",
  memorySwapUsedHelp:
    "Current usage summed across Windows page files. This is not committed memory or the amount written to disk over time.",
  battery: "Battery",
  network: "Network",
  /** The disk card (diskAvailable, capitalized). */
  available: "Available",
  disks: "Disks",
  power: "Power",
  connectedDevices: "Connected Devices",
  unavailable: "Sensors unavailable on this PC",
  back: "Back",
  usage: "Hardware usage",
  upFor: "Up for",
  measuring: "Measuring…",
  download: "Download",
  upload: "Upload",
  thisSession: "This session",
  used: "used",
  availableRow: "available",
  read: "Read",
  write: "Write",
  charge: "Charge",
  system: "System",
  adapter: "Adapter",
  charging: "Charging",
  onBattery: "On battery",
  pluggedIn: "Plugged in",
  powerUnavailable: "Power metrics unavailable on this PC",
  batteryTime: "Battery time remaining",
  calculating: "Calculating…",
  noDisks: "No mounted disks found.",
  noDevices: "No external devices connected",
  unnamedUsb: "USB Device",
  unnamedBluetooth: "Bluetooth Device",
  devicesConnected: (count: number) => (count === 1 ? "1 device connected" : `${count} devices connected`),
} as const;
