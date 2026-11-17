// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the shelf backend (src-tauri/src/files) sends: its items, as saved in
// shelf\items.json, and the ZIP job's state.

export type ItemKind = "file" | "text" | "link" | "batch";

export interface ShelfItem {
  id: string;
  kind: ItemKind;
  title: string;
  path?: string;
  fileId?: string;
  text?: string;
  url?: string;
  /** A pile's items. */
  children?: ShelfItem[];
  pinned?: boolean;
}

export type ZipFailure =
  | { kind: "cancelled" }
  | { kind: "inside" }
  | { kind: "exists"; name: string }
  | { kind: "io"; name: string; detail: string };

export interface ZipStatus {
  running: boolean;
  completed: number;
  total: number;
  outputs: string[];
  failure: ZipFailure | null;
  cancelled: boolean;
}

export interface FilesSnapshot {
  items: ShelfItem[];
  zip: ZipStatus;
}

export interface DragPhase {
  phase: "enter" | "leave" | "drop";
  /** Drop only: the shelf took it. */
  added: boolean;
}

export interface OpenWithApp {
  id: string;
  name: string;
  icon: string | null;
}
