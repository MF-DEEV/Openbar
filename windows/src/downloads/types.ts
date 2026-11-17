// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

export interface DownloadItem {
  id: string;
  name: string;
  path: string;
  source: string;
  received: number;
  total: number | null;
  speed: number;
  status: "downloading" | "completed" | "interrupted";
}
export interface DownloadSnapshot {
  items: DownloadItem[];
  folders: string[];
}
