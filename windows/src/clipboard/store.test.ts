// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
import type { ClipboardEntry, ClipboardSnapshot } from "./types";

const connection = vi.hoisted(() => ({
  receive: (_snapshot: ClipboardSnapshot) => {},
  cleared: [] as Record<string, number>[],
}));
vi.mock("../shared/bridge", () => ({
  bridge: {
    clipboard: {
      onHistory: async (handler: (snapshot: ClipboardSnapshot) => void) => {
        connection.receive = handler;
        return () => {};
      },
      snapshot: async () => ({ entries: [], captured: null }),
      clearRecent: async (confirmed: Record<string, number>) => {
        connection.cleared.push(confirmed);
      },
    },
  },
}));
import { subscribeClipboard, useClipboard } from "./store";
import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";

function demoEntry(id: string, extra: Partial<ClipboardEntry> = {}): ClipboardEntry {
  return {
    id,
    kind: "text",
    text: id,
    preview: id,
    cardPreview: id,
    copiedAt: 0,
    pinned: false,
    filePaths: [],
    imagePath: null,
    imageWidth: null,
    imageHeight: null,
    sourceExe: null,
    sourceName: null,
    ...extra,
  };
}

afterEach(() => {
  useIsland.getState().collapse();
  useIsland.getState().dismissNotice();
  useIsland.setState({ settings: DEFAULT_SETTINGS });
  useClipboard.setState({ entries: [], confirmClear: null, pinnedOnly: false });
  connection.cleared = [];
});

const on = { ...DEFAULT_SETTINGS, clipboardHistoryEnabled: true };
const snapshot = (captured: string | null): ClipboardSnapshot => ({ entries: [demoEntry("a")], captured });

describe("clipboard notice", () => {
  it("says Copied for a new copy while the island is closed", async () => {
    await subscribeClipboard();
    useIsland.setState({ settings: on });
    connection.receive(snapshot("a"));
    const notice = useIsland.getState().notice;
    expect(notice).toMatchObject({ kind: "clipboard", title: "Copied", detail: "Clipboard", priority: 0 });
    expect(notice?.duration).toBe(2.5);
    expect(notice?.opens).toBe("clipboard");
    expect(useClipboard.getState().entries).toHaveLength(1);
  });

  it("stays quiet for edits, while open, with the notice off or the section hidden", async () => {
    await subscribeClipboard();
    useIsland.setState({ settings: on });
    connection.receive(snapshot(null));
    expect(useIsland.getState().notice).toBeNull();
    useIsland.getState().open("clipboard");
    connection.receive(snapshot("a"));
    expect(useIsland.getState().notice).toBeNull();
    useIsland.getState().collapse();
    useIsland.setState({ settings: { ...on, clipboardNotice: false } });
    connection.receive(snapshot("a"));
    expect(useIsland.getState().notice).toBeNull();
    useIsland.setState({ settings: { ...on, hiddenModules: ["clipboard"] } });
    connection.receive(snapshot("a"));
    expect(useIsland.getState().notice).toBeNull();
  });
});

describe("clear unpinned", () => {
  it("asks first, then clears exactly the entries it counted", () => {
    useClipboard.setState({
      entries: [demoEntry("p", { pinned: true }), demoEntry("a", { copiedAt: 5 }), demoEntry("b", { copiedAt: 7 })],
    });
    useClipboard.getState().askClearRecent();
    expect(useClipboard.getState().confirmClear).toEqual({ a: 5, b: 7 });
    useClipboard.getState().answerClearRecent(true);
    expect(useClipboard.getState().confirmClear).toBeNull();
    expect(connection.cleared).toEqual([{ a: 5, b: 7 }]);
  });

  it("cancelling clears nothing, and with no recent entries nothing is asked", () => {
    useClipboard.setState({ entries: [demoEntry("a")] });
    useClipboard.getState().askClearRecent();
    useClipboard.getState().answerClearRecent(false);
    expect(connection.cleared).toEqual([]);
    useClipboard.setState({ entries: [demoEntry("p", { pinned: true })] });
    useClipboard.getState().askClearRecent();
    expect(useClipboard.getState().confirmClear).toBeNull();
  });
});
