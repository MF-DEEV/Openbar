// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The clipboard history as the Rust side sends it (clipboard/mod.rs View).

export type EntryKind = "text" | "image" | "files";

export interface ClipboardEntry {
  id: string;
  kind: EntryKind;
  /** The text, cut at 32,000 characters for search; empty for images and files. */
  text: string;
  /** One paragraph, at most 2,000 characters. */
  preview: string;
  /** The preview with its line breaks kept. */
  cardPreview: string;
  /** Milliseconds since the epoch. */
  copiedAt: number;
  pinned: boolean;
  filePaths: string[];
  /** The stored PNG of an image entry. */
  imagePath: string | null;
  imageWidth: number | null;
  imageHeight: number | null;
  /** The app the copy came from, when Windows could tell. */
  sourceExe: string | null;
  sourceName: string | null;
}

export interface ClipboardSnapshot {
  entries: ClipboardEntry[];
  /** The entry a new copy just added or refreshed. */
  captured: string | null;
}

/** What a paste did: typed into the previous app, only copied (an elevated or missing target), or nothing. */
export type PasteResult = "pasting" | "copied" | "failed";

export type CardSize = "compact" | "comfortable";
