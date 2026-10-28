// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Battery Saver halves the Clawd clock, as Low Power Mode does in
// NotchClawdView.updateLayers.

import { describe, expect, it, vi } from "vitest";
import { RATE } from "./sprite";

const power = vi.hoisted(() => ({
  handler: undefined as ((on: boolean) => void) | undefined,
  on: true as boolean | undefined,
}));

vi.mock("../../shared/bridge", () => ({
  bridge: {
    power: {
      saver: () => Promise.resolve(power.on),
      onSaver: (handler: (on: boolean) => void) => {
        power.handler = handler;
        return Promise.resolve(() => {});
      },
    },
  },
}));

describe("Clawd clock under Battery Saver", () => {
  it("starts from the state read at load and follows every change", async () => {
    const { clockPeriod } = await import("./Clawd");
    await vi.waitFor(() => expect(clockPeriod()).toBe(1000 / (RATE / 2)));

    power.handler!(false);
    expect(clockPeriod()).toBe(1000 / RATE);
    power.handler!(true);
    expect(clockPeriod()).toBe(1000 / (RATE / 2));
  });
});
