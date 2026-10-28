// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

const fake = vi.hoisted(() => ({
  bridge: {
    capture: { run: vi.fn() },
    toggles: { state: vi.fn(async () => undefined), run: vi.fn(async () => undefined) },
  } as Record<string, unknown>,
}));
vi.mock("../shared/bridge", () => ({ inTauri: false, bridge: fake.bridge }));

import { DEFAULT_SETTINGS } from "../island/settings";
import { useIsland } from "../island/store";
import { onPrefs, setPrefs } from "../shared/prefs";
import { noticeFor, screenshotName } from "./capture";
import { reorderTools, setToolHidden, toolLists, useTools } from "./store";

beforeAll(async () => {
  await onPrefs((p) => useIsland.setState({ settings: p.island }));
});

afterEach(async () => {
  await setPrefs({ island: { toolsHidden: null, toolsOrder: null, hiddenModules: null } });
  useTools.setState({ optionsItem: null, editing: false });
});

describe("the tools grid", () => {
  it("shows the Windows tools in the Mac's order and none of the Mac-only ones", () => {
    expect(toolLists(DEFAULT_SETTINGS).visible).toEqual([
      "keepAwake",
      "toggles",
      "micMute",
      "screenOCR",
      "colorPicker",
      "clipboard",
      "screenshot",
      "scratchpad",
    ]);
  });

  it("leaves out tools whose section is hidden", () => {
    const settings = { ...DEFAULT_SETTINGS, hiddenModules: ["scratchpad" as const] };
    expect(toolLists(settings).visible).not.toContain("scratchpad");
    expect(toolLists(settings).hidden).not.toContain("scratchpad");
  });
});

describe("edit mode", () => {
  it("removing a tool moves it to the Add back tray and adding returns it to its place", async () => {
    setToolHidden("colorPicker", true);
    await Promise.resolve();
    let lists = toolLists(useIsland.getState().settings);
    expect(lists.visible).not.toContain("colorPicker");
    expect(lists.hidden).toEqual(["colorPicker"]);

    setToolHidden("colorPicker", false);
    await Promise.resolve();
    lists = toolLists(useIsland.getState().settings);
    expect(lists.visible.indexOf("colorPicker")).toBe(4);
    expect(lists.hidden).toEqual([]);
  });

  it("hiding the tool whose options are open closes the card", () => {
    useTools.setState({ editing: true, optionsItem: "colorPicker" });
    setToolHidden("colorPicker", true);
    expect(useTools.getState().optionsItem).toBeNull();
  });

  it("reordering stores the whole order", async () => {
    reorderTools([
      "screenshot",
      "keepAwake",
      "toggles",
      "micMute",
      "screenOCR",
      "colorPicker",
      "clipboard",
      "scratchpad",
    ]);
    await Promise.resolve();
    const { settings } = useIsland.getState();
    expect(settings.toolsOrder[0]).toBe("screenshot");
    expect(toolLists(settings).visible.slice(0, 2)).toEqual(["screenshot", "keepAwake"]);
  });
});

describe("capture notices", () => {
  it("names screenshots the way Windows does", () => {
    expect(screenshotName(new Date(2026, 9, 7, 9, 3, 2))).toBe("Screenshot 2026-10-07 090302");
  });

  it("says where a screenshot went", () => {
    const notice = noticeFor({
      kind: "screenshot",
      path: "C:\\Users\\a\\OneDrive\\Pictures\\Screenshots\\x.png",
      folder: "C:\\Users\\a\\OneDrive\\Pictures\\Screenshots",
      copied: true,
      thumbnail: null,
    });
    expect(notice?.detail).toBe("Saved to Screenshots and copied");
    expect(noticeFor({ kind: "cancelled" })).toBeNull();
    expect(noticeFor({ kind: "color", value: "rgb(1, 2, 3)", hex: "#010203" })?.title).toBe("rgb(1, 2, 3)");
  });
});
