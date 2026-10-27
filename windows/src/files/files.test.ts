// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";
vi.mock("../shared/bridge", () => ({
  inTauri: false,
  bridge: {
    files: {
      onShelf: async () => () => {},
      onZip: async () => () => {},
      onDrag: async () => () => {},
      onMissing: async () => () => {},
      snapshot: async () => undefined,
    },
  },
}));
import { DEFAULT_SETTINGS } from "../island/settings";
import { presentation, geometryOf, useIsland } from "../island/store";
import {
  bandTargets,
  extended,
  NO_SELECTION,
  pruned,
  tileTargets,
  toggled,
  toggledPile,
  tooltipText,
  visibleItems,
} from "./model";
import { handleDrag } from "./register";
import { IDLE_ZIP, useFiles } from "./store";
import { middleTruncated } from "./Tile";
import type { ShelfItem } from "./types";

const file = (id: string, path = `C:\\x\\${id}.txt`): ShelfItem => ({ id, kind: "file", title: id, path });
const pile = (id: string, children: ShelfItem[]): ShelfItem => ({ id, kind: "batch", title: id, children });

const shelf: ShelfItem[] = [file("a"), pile("p", [file("p1"), file("p2", "C:\\x\\p2.png")]), file("b"), file("c")];

afterEach(() => {
  useFiles.setState({ items: [], zip: IDLE_ZIP, selection: NO_SELECTION, expanded: new Set(), arrivals: 0 });
  useIsland.setState({ settings: DEFAULT_SETTINGS, expanded: false, dropHint: false, selected: "controls" });
});

describe("selection", () => {
  it("toggles a tile on click and makes it the anchor", () => {
    let s = toggled(NO_SELECTION, "a");
    expect([...s.ids]).toEqual(["a"]);
    expect(s.anchor).toBe("a");
    s = toggled(s, "a");
    expect(s.ids.size).toBe(0);
    expect(s.anchor).toBe("a");
  });

  it("adds the range from the anchor on Shift+click, either way, keeping earlier picks", () => {
    const visible = ["a", "p", "b", "c"];
    let s = toggled(NO_SELECTION, "a");
    s = extended(s, visible, "b");
    expect([...s.ids].sort()).toEqual(["a", "b", "p"]);
    expect(s.anchor).toBe("b");
    // Backwards from the new anchor, unioned with what was there.
    s = toggled({ ids: new Set(["c"]), anchor: null }, "c");
    s = extended(s, visible, "p");
    expect([...s.ids].sort()).toEqual(["b", "c", "p"]);
    // Without an anchor, Shift+click picks just that tile.
    expect([...extended(NO_SELECTION, visible, "b").ids]).toEqual(["b"]);
  });

  it("acts on the selection when the clicked tile is in it, else on that tile alone", () => {
    const selection = new Set(["b", "p1"]);
    expect(tileTargets(shelf, selection, "b")).toEqual(["p1", "b"]);
    expect(tileTargets(shelf, selection, "c")).toEqual(["c"]);
    expect(bandTargets(shelf, new Set()).map((i) => i.id)).toEqual(["a", "p", "b", "c"]);
  });

  it("drives the store: click, then Shift+click across an open pile", () => {
    useFiles.getState().receive({ items: shelf, zip: IDLE_ZIP });
    useFiles.getState().togglePile("p");
    useFiles.getState().click("a", false);
    useFiles.getState().click("p2", true);
    expect([...useFiles.getState().selection.ids].sort()).toEqual(["a", "p", "p1", "p2"]);
  });
});

describe("piles", () => {
  it("spill their items after them while open", () => {
    expect(visibleItems(shelf, new Set()).map((i) => i.id)).toEqual(["a", "p", "b", "c"]);
    expect(visibleItems(shelf, new Set(["p"])).map((i) => i.id)).toEqual(["a", "p", "p1", "p2", "b", "c"]);
  });

  it("deselect what they hold when closed", () => {
    const open = toggledPile(shelf, new Set(), NO_SELECTION, "p");
    expect([...open.expanded]).toEqual(["p"]);
    const closed = toggledPile(shelf, open.expanded, { ids: new Set(["p1", "a"]), anchor: "p1" }, "p");
    expect(closed.expanded.size).toBe(0);
    expect([...closed.selection.ids]).toEqual(["a"]);
    expect(closed.selection.anchor).toBeNull();
    // A tile that isn't a pile doesn't open.
    expect(toggledPile(shelf, new Set(), NO_SELECTION, "a").expanded.size).toBe(0);
  });

  it("forget what left the shelf", () => {
    const after = pruned([file("a")], new Set(["p"]), { ids: new Set(["a", "p1"]), anchor: "p1" });
    expect(after.expanded.size).toBe(0);
    expect([...after.selection.ids]).toEqual(["a"]);
    expect(after.selection.anchor).toBeNull();
  });

  it("describe what they hold in the tooltip", () => {
    const mixed = pile("m", [file("x", "C:\\a.png"), file("y"), { id: "t", kind: "text", title: "t", text: " hi " }]);
    expect(tooltipText(mixed)).toBe("3 items: 1 image, 1 file, 1 note");
    expect(tooltipText(file("doc"), "PDF Document")).toBe("doc\nPDF Document");
    expect(tooltipText({ id: "t", kind: "text", title: "t", text: "x".repeat(600) })).toHaveLength(501);
  });
});

describe("titles", () => {
  const byLength = (text: string, room: number) => text.length <= room;
  it("cut long names in the middle so the extension stays", () => {
    expect(middleTruncated("short.txt", 20, byLength)).toBe("short.txt");
    const cut = middleTruncated("a-very-long-file-name-indeed.pdf", 16, byLength);
    expect(cut).toHaveLength(16);
    expect(cut.startsWith("a-very")).toBe(true);
    expect(cut.endsWith("ed.pdf")).toBe(true);
    expect(cut).toContain("…");
  });
});

describe("a drag over the island", () => {
  it("shows the drop hint on the closed island, then opens Files without the keyboard", () => {
    const before = useIsland.getState().focusRequest;
    handleDrag({ phase: "enter", added: false });
    expect(useIsland.getState().dropHint).toBe(true);
    const s = useIsland.getState();
    expect(presentation(s, geometryOf(s)).kind).toBe("dropHint");
    handleDrag({ phase: "drop", added: true });
    expect(useIsland.getState().dropHint).toBe(false);
    expect(useIsland.getState().expanded).toBe(true);
    expect(useIsland.getState().selected).toBe("files");
    expect(useIsland.getState().focusRequest).toBe(before);
  });

  it("drops the hint when the drag leaves, and opens Files at once with the hint off", () => {
    handleDrag({ phase: "enter", added: false });
    handleDrag({ phase: "leave", added: false });
    expect(useIsland.getState().dropHint).toBe(false);
    expect(useIsland.getState().expanded).toBe(false);
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, dragReveal: false } });
    handleDrag({ phase: "enter", added: false });
    expect(useIsland.getState().dropHint).toBe(false);
    expect(useIsland.getState().selected).toBe("files");
  });

  it("turns an open island to Files, and does nothing while Files is hidden", () => {
    useIsland.getState().open("music");
    handleDrag({ phase: "enter", added: false });
    expect(useIsland.getState().selected).toBe("files");
    useIsland.getState().collapse();
    useIsland.setState({ settings: { ...DEFAULT_SETTINGS, hiddenModules: ["files"] } });
    handleDrag({ phase: "enter", added: false });
    expect(useIsland.getState().dropHint).toBe(false);
    handleDrag({ phase: "drop", added: true });
    expect(useIsland.getState().expanded).toBe(false);
  });
});
