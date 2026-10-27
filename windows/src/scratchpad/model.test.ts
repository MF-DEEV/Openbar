// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import {
  MAX_PADS,
  addPad,
  applyRetention,
  exportFileName,
  initialDocument,
  listContinuation,
  nextPadName,
  removePad,
  renamePad,
  reorderPads,
  sanitizedName,
  sanitizedTextSize,
  toggleTaskAt,
  withSelectedText,
} from "./model";

describe("pads", () => {
  it("names new pads with the first free number", () => {
    expect(nextPadName("Scratchpad", [])).toBe("Scratchpad 1");
    expect(nextPadName("Scratchpad", ["Scratchpad 1", "Scratchpad 3"])).toBe("Scratchpad 2");
    expect(nextPadName("  ", [])).toBe("Scratchpad 1");
  });

  it("caps names at 40 characters and collapses spaces", () => {
    expect(sanitizedName("  a   b \n c ")).toBe("a b c");
    expect(sanitizedName("x".repeat(50))).toHaveLength(40);
  });

  it("stops at twelve pads", () => {
    let doc = initialDocument("Scratchpad", "p0");
    for (let i = 1; i < MAX_PADS; i++) doc = addPad(doc, "Scratchpad", `p${i}`)!;
    expect(doc.pads).toHaveLength(MAX_PADS);
    expect(doc.selectedId).toBe("p11");
    expect(addPad(doc, "Scratchpad")).toBeNull();
  });

  it("closing the chosen pad chooses its neighbour, and the last one stays", () => {
    const doc = addPad(addPad(initialDocument("S", "a"), "S", "b")!, "S", "c")!;
    expect(removePad({ ...doc, selectedId: "b" }, "b")?.selectedId).toBe("c");
    expect(removePad(doc, "c")?.selectedId).toBe("b");
    expect(removePad(initialDocument("S", "only"), "only")).toBeNull();
  });

  it("refuses an empty rename", () => {
    const doc = initialDocument("S", "a");
    expect(renamePad(doc, "a", "   ")).toBeNull();
    expect(renamePad(doc, "a", " Ideas ")?.pads[0]!.name).toBe("Ideas");
  });

  it("dates edits and forgets the date of an emptied pad", () => {
    const doc = withSelectedText(initialDocument("S", "a"), "hi", 1000);
    expect(doc.pads[0]!.modifiedAt).toBe(1000);
    expect(withSelectedText(doc, "", 2000).pads[0]!.modifiedAt).toBeUndefined();
  });

  it("reorders tabs and ignores an order that loses one", () => {
    const doc = addPad(initialDocument("S", "a"), "S", "b")!;
    expect(reorderPads(doc, ["b", "a"]).pads.map((p) => p.id)).toEqual(["b", "a"]);
    expect(reorderPads(doc, ["b"])).toBe(doc);
  });

  it("clears text left alone past the retention period, and only that", () => {
    const day = 86_400_000;
    let doc = withSelectedText(initialDocument("S", "a"), "old", 0);
    doc = withSelectedText({ ...addPad(doc, "S", "b")! }, "new", 6 * day);
    expect(applyRetention(doc, "never", 10 * day)).toBe(doc);
    const kept = applyRetention(doc, "week", 8 * day);
    expect(kept.pads.map((p) => p.text)).toEqual(["", "new"]);
    expect(applyRetention(doc, "month", 8 * day)).toBe(doc);
  });

  it("clamps the text size", () => {
    expect(sanitizedTextSize(30)).toBe(22);
    expect(sanitizedTextSize(9)).toBe(10);
    expect(sanitizedTextSize(Number.NaN)).toBe(13);
  });

  it("suggests an export name with the date", () => {
    expect(exportFileName("Notes: a/b", new Date(2026, 6, 17))).toBe("Notes- a-b 2026-07-17.txt");
  });
});

describe("editing", () => {
  it("continues bullets, numbers and task boxes", () => {
    expect(listContinuation("- milk", 6)).toEqual({ from: 6, to: 6, insert: "\n- " });
    expect(listContinuation("  9. nine", 9)).toEqual({ from: 9, to: 9, insert: "\n  10. " });
    expect(listContinuation("- [x] done", 10)).toEqual({ from: 10, to: 10, insert: "\n- [ ] " });
  });

  it("ends the list on an empty item and ignores plain lines", () => {
    expect(listContinuation("a\n- ", 4)).toEqual({ from: 2, to: 4, insert: "" });
    expect(listContinuation("plain", 5)).toBeNull();
  });

  it("flips the task box a preview click points at", () => {
    const text = "- [ ] one\n- [x] two";
    expect(toggleTaskAt(text, 0)).toBe("- [x] one\n- [x] two");
    expect(toggleTaskAt(text, 10)).toBe("- [ ] one\n- [ ] two");
  });
});
