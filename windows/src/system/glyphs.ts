// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Each metric's glyph (MetricDetailKind.symbolName, as Fluent-like Lucide icons).

import { Battery, Cable, Cpu, Gpu, HardDrive, type LucideIcon, MemoryStick, Network, PlugZap } from "lucide-react";
import type { MetricKind } from "./types";

export const GLYPHS: Record<MetricKind, LucideIcon> = {
  cpu: Cpu,
  gpu: Gpu,
  memory: MemoryStick,
  battery: Battery,
  network: Network,
  disk: HardDrive,
  power: PlugZap,
  connectedDevices: Cable,
};
