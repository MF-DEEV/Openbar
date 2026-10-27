// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Clipboard section's words, in the Mac's English (ClipboardFeatureStrings
// in FeatureStrings.swift, NotchClipboardSizeStrings); ⌘ becomes Ctrl.

export const C = {
  title: "Clipboard",
  search: "Search copied text",
  pinned: "Pinned",
  clearRecent: "Clear unpinned",
  clearRecentConfirm: (n: number) => `Clear unpinned (${n})?`,
  clearRecentConfirmMessage: "Pinned items stay, and so does anything copied after this. This can’t be undone.",
  cancel: "Cancel",
  empty: "No saved text",
  disabled: "Enable history to start saving copied text.",
  enable: "Enable",
  noResults: "No results",
  copy: "Copy",
  copied: "Copied",
  paste: "Paste",
  pin: "Pin",
  unpin: "Unpin",
  delete: "Delete item",
  moveUp: "Move up",
  moveDown: "Move down",
  clickRow: "Click to paste into the previous app",
  imageEntryLabel: "Image",
  fileCount: (n: number) => `${n} files`,
  /** The "⌘1" badge, Windows style. */
  shortcut: (n: number) => `Ctrl+${n}`,
} as const;
