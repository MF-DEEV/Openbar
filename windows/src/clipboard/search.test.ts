// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { cssColor, parseColor } from "./color";
import {
  filterEntries,
  highlight,
  normalized,
  searchableText,
  searchTokens,
  dimensions,
  fileName,
  isImageFile,
} from "./search";
import type { ClipboardEntry } from "./types";

function entry(id: string, text: string, extra: Partial<ClipboardEntry> = {}): ClipboardEntry {
  return {
    id,
    kind: "text",
    text,
    preview: text,
    cardPreview: text,
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

const ids = (list: ClipboardEntry[]) => list.map((e) => e.id);

describe("clipboard search", () => {
  it("folds case, accents and width, and line breaks to spaces", () => {
    expect(normalized("  Zażółć\nGĘŚLĄ\tJAŹŃ ")).toBe("zazołc gesla jazn");
    expect(normalized("ＡＢＣ１")).toBe("abc1");
    // No Turkish locale: a capital I still finds "istanbul".
    expect(normalized("ISTANBUL")).toBe("istanbul");
  });

  it("needs every word, and ranks exact, leading and whole-word matches first", () => {
    const list = [
      entry("contains", "a cat sat"),
      entry("prefix", "cats are great"),
      entry("exact", "cat"),
      entry("none", "dog"),
      entry("word", "the cat"),
    ];
    expect(ids(filterEntries(list, "cat"))).toEqual(["exact", "prefix", "contains", "word"]);
    expect(ids(filterEntries(list, "cat sat"))).toEqual(["contains"]);
    expect(ids(filterEntries(list, "  "))).toEqual(ids(list));
  });

  it("gives pins a nudge and otherwise keeps the history's order", () => {
    const list = [entry("a", "note one"), entry("b", "note two"), entry("p", "note three", { pinned: true })];
    expect(ids(filterEntries(list, "note"))).toEqual(["p", "a", "b"]);
  });

  it("finds pictures by label and size, and files by name", () => {
    const image = entry("img", "", { kind: "image", imageWidth: 640, imageHeight: 480 });
    const files = entry("files", "", { kind: "files", filePaths: ["C:\\x\\Report.pdf", "C:\\x\\shot.png"] });
    expect(searchableText(image)).toBe("Image png 640×480");
    expect(searchableText(files)).toBe("Image Report.pdf shot.png");
    expect(ids(filterEntries([image, files], "image"))).toEqual(["img", "files"]);
    expect(ids(filterEntries([image, files], "report"))).toEqual(["files"]);
  });

  it("marks matches in bold runs that keep the original text", () => {
    expect(highlight("Café au lait", searchTokens("cafe LAIT"))).toEqual([
      { text: "Café", match: true },
      { text: " au ", match: false },
      { text: "lait", match: true },
    ]);
    expect(highlight("abcabc", ["bc"])).toEqual([
      { text: "a", match: false },
      { text: "bc", match: true },
      { text: "a", match: false },
      { text: "bc", match: true },
    ]);
    expect(highlight("plain", [])).toEqual([{ text: "plain", match: false }]);
  });

  it("styles a long row only as far as it can show", () => {
    const long = "x".repeat(600) + "needle";
    const segments = highlight(long, ["needle"]);
    expect(segments).toHaveLength(1);
    expect(segments[0]!.text).toHaveLength(501);
    expect(segments[0]!.text.endsWith("…")).toBe(true);
  });
});

describe("color entries", () => {
  it("reads the CSS forms and SwiftUI's, and nothing inside longer text", () => {
    expect(parseColor("#f00")).toEqual({ red: 1, green: 0, blue: 0, alpha: 1 });
    expect(parseColor(" #FF000080 ")?.alpha).toBeCloseTo(0.502, 3);
    expect(parseColor("rgb(255, 0, 0)")).toEqual({ red: 1, green: 0, blue: 0, alpha: 1 });
    expect(parseColor("rgba(0 0 0 / 50%)")?.alpha).toBe(0.5);
    const green = parseColor("hsl(120deg, 100%, 50%)")!;
    expect([green.red, green.green, green.blue].map((v) => Math.round(v * 255))).toEqual([0, 255, 0]);
    expect(parseColor("Color(red: 0.2, green: 0.4, blue: 0.6)")?.blue).toBe(0.6);
    expect(parseColor("ff0000")).toBeNull();
    expect(parseColor("the color #f00")).toBeNull();
    expect(parseColor("rgb(300, 0, 0)")).toBeNull();
    expect(parseColor("#" + "f".repeat(100))).toBeNull();
    expect(cssColor({ red: 1, green: 0.5, blue: 0, alpha: 1 })).toBe("rgba(255, 128, 0, 1)");
  });
});

describe("file entries", () => {
  it("names a file from either separator and spots images by extension", () => {
    expect(fileName("C:\\Users\\me\\Shot.PNG")).toBe("Shot.PNG");
    expect(fileName("/tmp/notes.txt")).toBe("notes.txt");
    expect(isImageFile("C:\\Users\\me\\Shot.PNG")).toBe(true);
    expect(isImageFile("C:\\Users\\me\\photo.jpeg")).toBe(true);
    expect(isImageFile("C:\\Users\\me\\.png")).toBe(false);
    expect(isImageFile("C:\\png\\readme")).toBe(false);
  });

  it("writes an image's size only when both sides are known", () => {
    expect(dimensions({ imageWidth: 1920, imageHeight: 1080 })).toBe("1920×1080");
    expect(dimensions({ imageWidth: 1920, imageHeight: 0 })).toBe("");
    expect(dimensions({ imageWidth: null, imageHeight: null })).toBe("");
  });
});
