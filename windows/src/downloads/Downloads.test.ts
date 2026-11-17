// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
import type { DownloadItem, DownloadSnapshot } from "./types";
const connection = vi.hoisted(() => ({ receive: (_snapshot: DownloadSnapshot) => {} }));
vi.mock("../shared/bridge", () => ({
  bridge: {
    downloads: {
      onSnapshot: async (handler: (snapshot: DownloadSnapshot) => void) => {
        connection.receive = handler;
        return () => {};
      },
      snapshot: async () => ({ items: [], folders: [] }),
    },
  },
}));
import { arrival, folderNames, progress, subscribeDownloads, useDownloads } from "./Downloads";
import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";
const item: DownloadItem = {
  id: "fixture",
  name: "fixture.zip",
  path: "C:\\Downloads\\fixture.zip",
  source: "Files",
  received: 20,
  total: null,
  speed: 10,
  status: "downloading",
};
afterEach(() => {
  vi.useRealTimers();
  useIsland.getState().dismissNotice();
  useIsland.setState({ settings: DEFAULT_SETTINGS, activities: {}, inside: false });
  useDownloads.setState({ items: [], folders: [] });
});
describe("download activity", () => {
  it("keeps download details closed on hover even when other pages expand on hover", async () => {
    vi.useFakeTimers();
    await subscribeDownloads();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, opening: "hoverExpand" } });
    connection.receive({ items: [item], folders: [] });
    useIsland.getState().pointerEnter();
    vi.advanceTimersByTime(1000);
    expect(useIsland.getState().expanded).toBe(false);
  });
  it("stays compact while running and announces completion without opening the page", async () => {
    await subscribeDownloads();
    connection.receive({ items: [item], folders: [] });
    expect(useIsland.getState().notice).toBeNull();
    expect(useIsland.getState().expanded).toBe(false);
    expect(useIsland.getState().activities.downloads).toBeTruthy();
    connection.receive({ items: [{ ...item, total: 100, received: 40 }], folders: [] });
    expect(progress(useDownloads.getState().items[0]!)).toBe(40);
    expect(useIsland.getState().notice).toBeNull();
    connection.receive({ items: [{ ...item, total: 100, received: 100, status: "completed" }], folders: [] });
    expect(useDownloads.getState().items[0]?.status).toBe("completed");
    expect(useIsland.getState().notice).toMatchObject({
      kind: "downloads",
      title: "Download complete",
      detail: "fixture.zip",
      priority: 0,
      duration: 6,
      opens: "downloads",
      mascot: "celebrate",
    });
    expect(useIsland.getState().expanded).toBe(false);
    expect(useIsland.getState().activities.downloads).toBeUndefined();
  });
  it("respects disabled downloads and never substitutes a made-up percentage", async () => {
    await subscribeDownloads();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, downloadProgress: false } });
    connection.receive({ items: [item], folders: [] });
    expect(useIsland.getState().notice).toBeNull();
    expect(useIsland.getState().activities.downloads).toBeUndefined();
    expect(progress(item)).toBeNull();
    expect(progress({ ...item, total: 10, received: 20 })).toBe(100);
  });
  it("announces a file once, and again only when it is downloaded anew", async () => {
    await subscribeDownloads();
    const done = { ...item, total: 100, received: 100, status: "completed" as const };
    connection.receive({ items: [done], folders: [] });
    expect(useIsland.getState().notice?.detail).toBe("fixture.zip");
    useIsland.getState().dismissNotice();
    connection.receive({ items: [done], folders: [] });
    expect(useIsland.getState().notice).toBeNull();
    connection.receive({ items: [item], folders: [] });
    expect(useIsland.getState().notice).toBeNull();
    connection.receive({ items: [done], folders: [] });
    expect(useIsland.getState().notice?.detail).toBe("fixture.zip");
  });
  it("says nothing about what was already saved, or when something stops", () => {
    const done = { ...item, status: "completed" as const };
    expect(arrival(null, [done])).toBeNull();
    expect(arrival(new Set([item.id]), [done])).toBeNull();
    expect(arrival(new Set(), [{ ...item, status: "interrupted" }])).toBeNull();
    expect(arrival(new Set(), [item])).toBeNull();
    expect(arrival(new Set(), [done])).toBe(done);
  });
  it("gives no notice when downloads are turned off", async () => {
    await subscribeDownloads();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, downloadProgress: false } });
    connection.receive({ items: [{ ...item, status: "completed" }], folders: [] });
    expect(useIsland.getState().notice).toBeNull();
  });
  it("names folders by their last part, and in full where two share one", () => {
    expect(folderNames(["C:\\Users\\me\\Downloads"])).toEqual(["Downloads"]);
    expect(folderNames(["D:\\"])).toEqual(["D:"]);
    expect(folderNames(["C:\\Users\\me\\Downloads", "D:\\downloads", "E:\\Games\\"])).toEqual([
      "C:\\Users\\me\\Downloads",
      "D:\\downloads",
      "Games",
    ]);
  });
});
