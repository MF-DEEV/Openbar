// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.3.0") }));

import { useIsland } from "../island/store";
import { automaticCheck } from "./register";
import { useUpdates } from "./store";

const islandInitial = useIsland.getState();
const updatesInitial = useUpdates.getState();
const feed = (...tags: string[]) => tags.map((tag_name) => ({ tag_name }));

function serve(...tags: string[]) {
  const fetch = vi.fn(async () => new Response(JSON.stringify(feed(...tags))));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

/** The tests run without a browser: a page's storage and `window.open`, stood in for. */
function browser() {
  const kept = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, value),
  });
  const open = vi.fn();
  vi.stubGlobal("window", { open });
  return open;
}

let open: ReturnType<typeof browser>;
beforeEach(() => {
  vi.useFakeTimers();
  useIsland.setState(islandInitial, true);
  useUpdates.setState(updatesInitial, true);
  open = browser();
});
afterEach(() => {
  useIsland.getState().dismissNotice();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("automaticCheck", () => {
  it("raises a notice the first time a version is found, and a tap opens its page", async () => {
    serve("windows-v0.4.0");
    await automaticCheck();
    const notice = useIsland.getState().notice;
    expect(notice).toMatchObject({
      id: "update:0.4.0",
      kind: "update",
      title: "Openbar update",
      detail: "Update available: 0.4.0",
    });
    useIsland.getState().clickClosed();
    expect(open).toHaveBeenCalledWith("https://github.com/MF-DEEV/Openbar/releases/tag/windows-v0.4.0");
    expect(useIsland.getState()).toMatchObject({ expanded: false, notice: null });
  });

  it("does not repeat the notice for a version it has shown, but shows the next", async () => {
    serve("windows-v0.4.0");
    await automaticCheck();
    useIsland.getState().dismissNotice();
    await automaticCheck();
    expect(useIsland.getState().notice).toBeNull();
    serve("windows-v0.4.0", "windows-v0.5.0");
    await automaticCheck();
    expect(useIsland.getState().notice?.id).toBe("update:0.5.0");
  });

  it("asks nothing with the check turned off or the island off", async () => {
    const fetch = serve("windows-v0.4.0");
    useIsland.setState({ settings: { ...islandInitial.settings, checkUpdates: false } });
    await automaticCheck();
    useIsland.setState({ settings: { ...islandInitial.settings, enabled: false } });
    await automaticCheck();
    expect(fetch).not.toHaveBeenCalled();
    expect(useIsland.getState().notice).toBeNull();
  });

  it("raises no notice when nothing is newer or the check fails", async () => {
    serve("windows-v0.3.0");
    await automaticCheck();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("", { status: 500 })),
    );
    await automaticCheck();
    expect(useIsland.getState().notice).toBeNull();
  });

  it("is on by default", () => {
    expect(islandInitial.settings.checkUpdates).toBe(true);
  });
});

describe("the Settings button", () => {
  it("checks without raising a notice", async () => {
    serve("windows-v0.4.0");
    expect(await useUpdates.getState().check()).toBe("available");
    expect(useIsland.getState().notice).toBeNull();
  });
});
