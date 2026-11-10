// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { describe, expect, it } from "vitest";
import { makeGeometry } from "./geometry";
import { roomHeight } from "./room";
import { DEFAULT_SETTINGS } from "./settings";

const screen = { width: 1920, height: 1080 };
const notch = makeGeometry({ screen, devicePixelRatio: 1, silhouette: "notch", layout: "compact", notchHeight: 24 });

describe("room for the island", () => {
  it("keeps the closed island's height on Linux", () => {
    expect(roomHeight(DEFAULT_SETTINGS, notch, true)).toBe(24);
  });

  it("is on by default", () => {
    expect(DEFAULT_SETTINGS.makeRoom).toBe(true);
  });

  it("keeps nothing elsewhere, when switched off, with the island off, or hidden until hover", () => {
    expect(roomHeight(DEFAULT_SETTINGS, notch, false)).toBeNull();
    expect(roomHeight({ ...DEFAULT_SETTINGS, makeRoom: false }, notch, true)).toBeNull();
    expect(roomHeight({ ...DEFAULT_SETTINGS, enabled: false }, notch, true)).toBeNull();
    expect(roomHeight({ ...DEFAULT_SETTINGS, opening: "hiddenUntilHover" }, notch, true)).toBeNull();
    expect(roomHeight({ ...DEFAULT_SETTINGS, opening: "hoverExpand" }, notch, true)).toBe(24);
  });

  it("counts a floating capsule's drop", () => {
    const capsule = makeGeometry({
      screen,
      devicePixelRatio: 1,
      silhouette: "capsule",
      layout: "compact",
      barHeight: 32,
      capsuleFit: { width: 0, height: 0, drop: 6 },
    });
    expect(roomHeight(DEFAULT_SETTINGS, capsule, true)).toBe(Math.ceil(capsule.stripHeight + 6));
  });

  it("rounds a fraction of a pixel up", () => {
    expect(roomHeight(DEFAULT_SETTINGS, { stripHeight: 23.5, floatingDrop: 0 }, true)).toBe(24);
    expect(roomHeight(DEFAULT_SETTINGS, { stripHeight: Number.NaN, floatingDrop: 0 }, true)).toBeNull();
  });
});
