// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { imageFiles, isImagePasteShortcut } from "./paste";

function clipboard(files: File[], items: Partial<DataTransferItem>[] = []) {
  return { files, items } as unknown as Pick<DataTransfer, "files" | "items">;
}

it("leaves plain-text Ctrl+V to the browser", () => {
  expect(imageFiles(clipboard([], [{ kind: "string", type: "text/plain" }]))).toEqual([]);
  expect(
    isImagePasteShortcut({
      key: "v",
      ctrlKey: true,
      altKey: false,
      metaKey: false,
      shiftKey: false,
      repeat: false,
      isComposing: false,
    }),
  ).toBe(false);
});

it("accepts image files supplied by a real paste event", () => {
  const png = { type: "image/png", name: "image.png" } as File;
  const text = { type: "text/plain", name: "notes.txt" } as File;
  expect(imageFiles(clipboard([png, text]))).toEqual([png]);
});

it("reads clipboard image items when the file list is empty, without duplicates", () => {
  const png = { type: "image/png", name: "image.png" } as File;
  const item = { kind: "file", type: "image/png", getAsFile: () => png };
  expect(imageFiles(clipboard([], [item]))).toEqual([png]);
  expect(imageFiles(clipboard([png], [item]))).toEqual([png]);
  expect(imageFiles(clipboard([], [{ ...item, getAsFile: () => null }]))).toEqual([]);
});

it("recognizes Alt+V without interfering with Ctrl+V or key repeats", () => {
  const event = {
    key: "v",
    altKey: true,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
  };
  expect(isImagePasteShortcut(event)).toBe(true);
  expect(isImagePasteShortcut({ ...event, key: "V" })).toBe(true);
  for (const changes of [
    { ctrlKey: true },
    { metaKey: true },
    { shiftKey: true },
    { repeat: true },
    { isComposing: true },
  ]) {
    expect(isImagePasteShortcut({ ...event, ...changes })).toBe(false);
  }
});
