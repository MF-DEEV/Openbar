// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import type { AudioApp } from "../audio/types";
import {
  arrange,
  type Arrangement,
  cycleOrder,
  listedApps,
  move,
  moveBy,
  neighbor,
  ordered,
  togglePin,
} from "./arrangement";

const none: Arrangement = { order: [], pinned: [] };
const app = (key: string, extra: Partial<AudioApp> = {}): AudioApp => ({
  key,
  path: null,
  name: key,
  volume: 1,
  muted: false,
  playing: false,
  system: false,
  ...extra,
});

describe("mixer arrangement", () => {
  it("puts pinned apps first, then the remembered order, then the rest as they came", () => {
    const a: Arrangement = { order: ["c", "b"], pinned: ["d"] };
    expect(ordered(a, ["a", "b", "c", "d", "e"], (x) => x)).toEqual(["d", "c", "b", "a", "e"]);
  });

  it("pins and unpins", () => {
    const pinned = togglePin(none, "a");
    expect(pinned.pinned).toEqual(["a"]);
    expect(togglePin(pinned, "a").pinned).toEqual([]);
    expect(togglePin(none, "")).toBe(none);
  });

  it("moves only within the pin group", () => {
    const a: Arrangement = { order: [], pinned: ["p"] };
    const visible = ["p", "a", "b"];
    expect(neighbor(a, "a", -1, visible)).toBeNull();
    expect(neighbor(a, "a", 1, visible)).toBe("b");
    expect(move(a, "b", "p", false, visible)).toBe(a);
    const moved = moveBy(a, "b", -1, visible);
    expect(ordered(moved, visible, (x) => x)).toEqual(["p", "b", "a"]);
  });

  it("keeps hidden apps' remembered places when the visible ones move", () => {
    const a: Arrangement = { order: ["a", "gone", "b"], pinned: [] };
    const moved = move(a, "b", "a", false, ["a", "b"]);
    expect(moved.order).toEqual(["b", "gone", "a"]);
  });

  it("takes a dragged order into the remembered one", () => {
    const a: Arrangement = { order: ["u1", "p1", "u2"], pinned: ["p1"] };
    const visible = ordered(a, ["u1", "p1", "u2"], (x) => x);
    expect(visible).toEqual(["p1", "u1", "u2"]);
    const next = arrange(a, visible, ["p1", "u2", "u1"]);
    expect(ordered(next, ["u1", "p1", "u2"], (x) => x)).toEqual(["p1", "u2", "u1"]);
  });

  it("lists apps without the hidden ones, and with hide-inactive only what plays or is set", () => {
    const apps = [
      app("idle"),
      app("playing", { playing: true }),
      app("quiet", { volume: 0.4 }),
      app("muted", { muted: true }),
      app("hidden", { playing: true }),
    ];
    const hidden = { hidden: "Hidden" };
    expect(listedApps(apps, none, hidden, false).map((a) => a.key)).toEqual(["idle", "playing", "quiet", "muted"]);
    expect(listedApps(apps, none, hidden, true).map((a) => a.key)).toEqual(["playing", "quiet", "muted"]);
  });
});

describe("output switcher cycle", () => {
  it("follows the outputs' order and keeps disconnected choices last", () => {
    const outputs = [{ id: "a" }, { id: "b" }, { id: "c" }];
    expect(cycleOrder(["c", "gone", "a"], outputs)).toEqual(["a", "c", "gone"]);
    expect(cycleOrder([], outputs)).toEqual([]);
  });
});
