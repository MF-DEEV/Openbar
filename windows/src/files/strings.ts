// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, tf, getLanguage, pluralForm, type StringKey } from "../i18n";

// The Files section's words, as the Mac says them (Localization.swift
// shelf*, NotchFilesStrings, NotchStrings.dropHint); Finder becomes File
// Explorer.

import type { ZipFailure } from "./types";

/** "%d image" / "%d images": English needs only one and many. */
const counted = (n: number, kind: "Image" | "File" | "Note" | "Link") => {
  const form = pluralForm(getLanguage(), n);
  const suffix = form === "one" ? "Singular" : form === "few" ? "Few" : "Plural";
  return tf(`s.shelfTooltip${kind}${suffix}` as StringKey, n);
};

export const S = {
  get dropHint() {
    return t("notch.dropHint");
  },
  get hint() {
    return t("s.shelfHint");
  },
  selected: (n: number) => tf("s.shelfSelectedFormat", n),
  get clearAll() {
    return t("s.shelfClearAll");
  },
  get cancel() {
    return t("clipboard.cancel");
  },
  get removeSelected() {
    return t("s.shelfRemoveSelected");
  },
  get remove() {
    return t("appUpdates.removeRule");
  },
  get share() {
    return t("s.shelfActionShare");
  },
  get archive() {
    return t("notchFiles.archive");
  },
  get archiveHint() {
    return t("notchFiles.archiveHint");
  },
  progress: (done: number, total: number) => tf("windows.files.progress", done, total),
  get saved() {
    return t("notchFiles.saved");
  },
  get showInExplorer() {
    return t("windows.files.showInExplorer");
  },
  get cancelled() {
    return t("s.mediaCancelled");
  },
  get pin() {
    return t("s.shelfActionPin");
  },
  get unpin() {
    return t("s.shelfActionUnpin");
  },
  get pinned() {
    return t("clipboard.pinned");
  },
  get open() {
    return t("s.shelfActionOpen");
  },
  get openWith() {
    return t("s.shelfActionOpenWith");
  },
  get expand() {
    return t("windows.files.expand");
  },
  get collapse() {
    return t("windows.files.collapse");
  },
  get fileMissing() {
    return t("s.shelfFileMissing");
  },
  failure(failure: ZipFailure): string {
    switch (failure.kind) {
      case "inside":
        return t("windows.files.failure");
      case "exists":
        return tf("windows.files.failuref3128c", failure.name);
      case "io":
        return tf("windows.files.failure82d8c5", failure.name, failure.detail);
      case "cancelled":
        return t("s.mediaCancelled");
    }
  },
  /** A pile's tooltip: "5 items: 2 images, 3 files" (ShelfTooltipSupport.text(forPile:)). */
  pile(counts: { images: number; files: number; notes: number; links: number }): string {
    const total = counts.images + counts.files + counts.notes + counts.links;
    const parts = [
      counts.images ? counted(counts.images, "Image") : "",
      counts.files ? counted(counts.files, "File") : "",
      counts.notes ? counted(counts.notes, "Note") : "",
      counts.links ? counted(counts.links, "Link") : "",
    ].filter(Boolean);
    const items = tf(
      pluralForm(getLanguage(), total) === "few" ? "s.shelfTooltipItemsFew" : "s.shelfTooltipItemsFormat",
      total,
    );
    return parts.length ? tf("windows.files.pile201b2f", items, parts.join(", ")) : items;
  },
};
