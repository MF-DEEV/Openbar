// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Files section's words, as the Mac says them (Localization.swift
// shelf*, NotchFilesStrings, NotchStrings.dropHint); Finder becomes File
// Explorer.

import type { ZipFailure } from "./types";

/** "%d image" / "%d images": English needs only one and many. */
const counted = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export const S = {
  dropHint: "Drop files here",
  hint: "Click to select. Drag out to use or right-click for more actions.",
  selected: (n: number) => `${n} selected`,
  clearAll: "Clear all",
  cancel: "Cancel",
  removeSelected: "Remove selected",
  remove: "Remove",
  share: "Share",
  archive: "Create ZIP",
  archiveHint: "Each selected item is saved as a separate ZIP. Originals stay unchanged.",
  progress: (done: number, total: number) => `${done}/${total}`,
  saved: "Saved",
  showInExplorer: "Show in File Explorer",
  cancelled: "Cancelled.",
  pin: "Pin",
  unpin: "Unpin",
  pinned: "Pinned",
  open: "Open",
  openWith: "Open With",
  expand: "Show the pile's items",
  collapse: "Hide the pile's items",
  fileMissing: "The file no longer exists",
  failure(failure: ZipFailure): string {
    switch (failure.kind) {
      case "inside":
        return "A ZIP can't be saved inside an item it contains. Choose another place.";
      case "exists":
        return `“${failure.name}” already exists. Choose another name.`;
      case "io":
        return `Couldn't create the ZIP: “${failure.name}”. ${failure.detail}`;
      case "cancelled":
        return "Cancelled.";
    }
  },
  /** A pile's tooltip: "5 items: 2 images, 3 files" (ShelfTooltipSupport.text(forPile:)). */
  pile(counts: { images: number; files: number; notes: number; links: number }): string {
    const total = counts.images + counts.files + counts.notes + counts.links;
    const parts = [
      counts.images ? counted(counts.images, "image", "images") : "",
      counts.files ? counted(counts.files, "file", "files") : "",
      counts.notes ? counted(counts.notes, "note", "notes") : "",
      counts.links ? counted(counts.links, "link", "links") : "",
    ].filter(Boolean);
    const items = `${total} items`;
    return parts.length ? `${items}: ${parts.join(", ")}` : items;
  },
};
