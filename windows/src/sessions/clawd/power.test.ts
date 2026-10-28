// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Energy saver halves the Clawd clock, as Low Power Mode does in
// NotchClawdView.updateLayers.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { RATE } from "./sprite";

const power = vi.hoisted(() => ({
  handler: undefined as ((on: boolean) => void) | undefined,
  on: true as boolean | undefined,
  calls: [] as string[],
  read: undefined as (() => Promise<boolean | undefined>) | undefined,
}));

vi.mock("../../shared/bridge", () => ({
  bridge: {
    power: {
      saver: () => {
        power.calls.push("read");
        return power.read ? power.read() : Promise.resolve(power.on);
      },
      onSaver: (handler: (on: boolean) => void) => {
        power.calls.push("listen");
        power.handler = handler;
        return Promise.resolve(() => {});
      },
    },
  },
}));

beforeEach(() => {
  // Each test loads Clawd afresh, so its reading at load runs again.
  vi.resetModules();
  power.handler = undefined;
  power.on = true;
  power.calls.length = 0;
  power.read = undefined;
});

describe("Clawd clock under Energy saver", () => {
  it("starts from the state read at load and follows every change", async () => {
    const { clockPeriod } = await import("./Clawd");
    await vi.waitFor(() => expect(clockPeriod()).toBe(1000 / (RATE / 2)));

    power.handler!(false);
    expect(clockPeriod()).toBe(1000 / RATE);
    power.handler!(true);
    expect(clockPeriod()).toBe(1000 / (RATE / 2));
  });

  it("listens before it reads, so a change in between isn't lost", async () => {
    await import("./Clawd");
    await vi.waitFor(() => expect(power.calls).toHaveLength(2));
    expect(power.calls).toEqual(["listen", "read"]);
  });

  it("lets an event heard first win over a reading that answers after it", async () => {
    let answer!: (on: boolean) => void;
    const reading = new Promise<boolean>((resolve) => (answer = resolve));
    power.read = () => reading;
    const { clockPeriod } = await import("./Clawd");
    await vi.waitFor(() => expect(power.calls).toEqual(["listen", "read"]));
    power.handler!(true);
    answer(false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clockPeriod()).toBe(1000 / (RATE / 2));
  });

  it("keeps the full rate when there's no reading (outside the app)", async () => {
    power.on = undefined;
    const { clockPeriod } = await import("./Clawd");
    await vi.waitFor(() => expect(power.calls).toEqual(["listen", "read"]));
    // Lets the reading's promise settle before looking.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(clockPeriod()).toBe(1000 / RATE);
  });
});
