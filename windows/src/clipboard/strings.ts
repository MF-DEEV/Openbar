// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, tf } from "../i18n/index";

// The Clipboard section's words, in the Mac's English (ClipboardFeatureStrings
// in FeatureStrings.swift, NotchClipboardSizeStrings); ⌘ becomes Ctrl.

export const C = {
  get title() {
    return t("clipboard.title");
  },
  get search() {
    return t("clipboard.search");
  },
  get pinned() {
    return t("clipboard.pinned");
  },
  get clearRecent() {
    return t("clipboard.clearRecent");
  },
  clearRecentConfirm: (n: number) => tf("clipboard.clearRecentConfirmFormat", n),
  get clearRecentConfirmMessage() {
    return t("clipboard.clearRecentConfirmMessage");
  },
  get cancel() {
    return t("clipboard.cancel");
  },
  get empty() {
    return t("clipboard.empty");
  },
  get disabled() {
    return t("clipboard.disabled");
  },
  get enable() {
    return t("windows.clipboard.enable");
  },
  get noResults() {
    return t("clipboard.noResults");
  },
  get copy() {
    return t("clipboard.copy");
  },
  get copied() {
    return t("clipboard.copied");
  },
  get paste() {
    return t("s.menuPaste");
  },
  get pin() {
    return t("clipboard.pin");
  },
  get unpin() {
    return t("clipboard.unpin");
  },
  get delete() {
    return t("clipboard.delete");
  },
  get moveUp() {
    return t("clipboard.moveUp");
  },
  get moveDown() {
    return t("clipboard.moveDown");
  },
  get imageEntryLabel() {
    return t("clipboard.imageEntryLabel");
  },
  fileCount: (n: number) => tf("clipboard.fileCountFormat", n),
  /** The "⌘1" badge, Windows style. */
  shortcut: (n: number) => tf("windows.clipboard.shortcut", n),
} as const;
