// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
import { beforeEach, describe, expect, it, vi } from "vitest";

const noted: string[] = [];
vi.mock("./usageStats", () => ({ noteSectionOpened: (id: string) => noted.push(id) }));

const { useIsland } = await import("./store");
const initial = useIsland.getState();

beforeEach(() => {
  useIsland.setState(initial, true);
  noted.length = 0;
});

describe("usage stats: section visits (#333)", () => {
  it("counts opening the island and switching sections, once each", () => {
    const island = useIsland.getState();
    island.open("mixer");
    island.open("mixer");
    useIsland.getState().select("music");
    useIsland.getState().select("music");
    useIsland.getState().collapse();
    useIsland.getState().open("music", { byHover: true });
    expect(noted).toEqual(["mixer", "music", "music"]);
  });

  it("leaves out the island opening by itself and the sections gallery", () => {
    useIsland.getState().open("sessions", { takeFocus: false, automatic: true });
    useIsland.getState().collapse();
    useIsland.getState().open("controls", { sections: true });
    expect(noted).toEqual([]);
    useIsland.getState().select("controls");
    expect(noted).toEqual(["controls"]);
  });

  it("is on by default in the settings", () => {
    expect(useIsland.getState().settings.shareUsageStats).toBe(true);
  });
});
