// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  clipboardKey,
  highlightFor,
  type KeyInput,
  openingScroll,
  openOffset,
  pastePlace,
  pasteTarget,
  restingHighlight,
  searchHighlight,
  steppedItem,
} from "./keys";

const key = (k: string, extra: Partial<KeyInput> = {}): KeyInput => ({
  key: k,
  code: "",
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  isComposing: false,
  ...extra,
});

describe("clipboard keys", () => {
  it("arrows move, Enter pastes, and Shift leaves both to the field", () => {
    expect(clipboardKey(key("ArrowDown"))).toEqual({ kind: "move", backwards: false });
    expect(clipboardKey(key("ArrowUp"))).toEqual({ kind: "move", backwards: true });
    expect(clipboardKey(key("Enter"))).toEqual({ kind: "paste" });
    expect(clipboardKey(key("ArrowDown", { shiftKey: true }))).toBeNull();
    expect(clipboardKey(key("Enter", { shiftKey: true }))).toBeNull();
  });

  it("printable characters and composition reveal the search; modifiers and spaces don't", () => {
    expect(clipboardKey(key("a"))).toEqual({ kind: "reveal" });
    expect(clipboardKey(key("Ż", { shiftKey: true }))).toEqual({ kind: "reveal" });
    expect(clipboardKey(key("Dead"))).toEqual({ kind: "reveal" });
    expect(clipboardKey(key("Process"))).toEqual({ kind: "reveal" });
    expect(clipboardKey(key("ArrowDown", { isComposing: true }))).toEqual({ kind: "reveal" });
    expect(clipboardKey(key(" "))).toBeNull();
    expect(clipboardKey(key("Tab"))).toBeNull();
    expect(clipboardKey(key("F5"))).toBeNull();
    expect(clipboardKey(key("c", { ctrlKey: true }))).toBeNull();
    expect(clipboardKey(key("a", { altKey: true }))).toBeNull();
  });

  it("Ctrl+1–9 read the physical digit row and the keypad", () => {
    expect(pastePlace(key("1", { ctrlKey: true, code: "Digit1" }))).toBe(0);
    // An AZERTY layout's top row types "&" but is still Digit1.
    expect(pastePlace(key("&", { ctrlKey: true, code: "Digit1" }))).toBe(0);
    expect(pastePlace(key("9", { ctrlKey: true, code: "Numpad9" }))).toBe(8);
    expect(pastePlace(key("0", { ctrlKey: true, code: "Digit0" }))).toBeNull();
    expect(pastePlace(key("1", { code: "Digit1" }))).toBeNull();
    expect(pastePlace(key("1", { ctrlKey: true, altKey: true, code: "Digit1" }))).toBeNull();
    expect(pastePlace(key("!", { ctrlKey: true, shiftKey: true, code: "Digit1" }))).toBeNull();
  });
});

describe("clipboard highlight", () => {
  const ids = ["a", "b", "c"];

  it("steps without wrapping, from the top on the first press", () => {
    expect(steppedItem(null, ids, false)).toBe("a");
    expect(steppedItem(null, ids, true)).toBe("a");
    expect(steppedItem("a", ids, true)).toBe("a");
    expect(steppedItem("b", ids, false)).toBe("c");
    expect(steppedItem("c", ids, false)).toBe("c");
    expect(steppedItem("gone", ids, false)).toBe("a");
    expect(steppedItem("a", [], false)).toBeNull();
  });

  it("Enter falls back to the first visible entry when the highlight is stale", () => {
    expect(pasteTarget("b", ids)).toBe("b");
    expect(pasteTarget("gone", ids)).toBe("a");
    expect(pasteTarget(null, [])).toBeNull();
  });

  it("a search keeps a listed row, else starts from its top result; an empty one waits", () => {
    expect(searchHighlight("b", ids, "x")).toBe("b");
    expect(searchHighlight("gone", ids, "x")).toBe("a");
    expect(searchHighlight("gone", ids, "  ")).toBeNull();
  });

  it("opening rests on the entry copied last: the first unpinned one", () => {
    const entries = [
      { id: "p", pinned: true },
      { id: "r", pinned: false },
    ];
    expect(restingHighlight(entries)).toBe("r");
    expect(restingHighlight([{ id: "p", pinned: true }])).toBe("p");
    expect(restingHighlight([])).toBeNull();
    expect(highlightFor(null, entries, "")).toBe("r");
    expect(highlightFor("p", entries, "")).toBe("p");
  });

  it("an open entry stays inside the list and leaves with its row", () => {
    // Room below: stays at its row's top.
    expect(openOffset(0, 52, 120, 180)).toBe(0);
    // Near the bottom: moves up just enough.
    expect(openOffset(120, 52, 120, 180)).toBe(-60);
    // Row partly above the top: moves down to the list's top.
    expect(openOffset(-20, 52, 120, 180)).toBe(20);
    // Row scrolled away above: never past the row's edge.
    expect(openOffset(-100, 52, 120, 180)).toBe(52);
    // Taller than the list: pinned to the top of the list from its row.
    expect(openOffset(60, 52, 180, 180)).toBe(-60);
  });

  it("opening scrolls by whole rows only when half a compact row wouldn't show", () => {
    // Comfortable rows, 104 + 8, in 264: row 2 starts at 224, 40 of it shows.
    expect(openingScroll(1, 104, 8, 26, 264)).toBe(0);
    expect(openingScroll(2, 104, 8, 26, 264)).toBe(0);
    expect(openingScroll(3, 104, 8, 26, 264)).toBe(1);
    expect(openingScroll(5, 52, 8, 26, 180)).toBe(3);
    expect(openingScroll(-1, 52, 8, 26, 180)).toBe(0);
  });
});
