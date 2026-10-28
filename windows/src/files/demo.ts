// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample shelf for designing in a plain browser (island.html?files).

import type { FilesSnapshot, ShelfItem } from "./types";

const file = (id: string, path: string, pinned = false): ShelfItem => ({
  id,
  kind: "file",
  title: path.split("\\").pop()!,
  path,
  pinned,
});

export function demoShelf(): FilesSnapshot {
  return {
    items: [
      file("a", "C:\\Users\\Dawid\\Desktop\\Quarterly report — final version (signed).pdf", true),
      file("b", "C:\\Users\\Dawid\\Pictures\\holiday.jpg"),
      {
        id: "c",
        kind: "text",
        title: "Meeting notes for Thursday",
        text: "Meeting notes for Thursday\n- budget\n- hiring",
      },
      { id: "d", kind: "link", title: "github.com", url: "https://github.com/MF-DEEV/vorssaint" },
      {
        id: "e",
        kind: "batch",
        title: "design.fig +2",
        children: [
          file("e1", "C:\\Work\\design.fig"),
          file("e2", "C:\\Work\\logo.png"),
          file("e3", "C:\\Work\\brief.docx"),
        ],
      },
      file("f", "C:\\Work\\Project"),
    ],
    zip: {
      running: false,
      completed: 2,
      total: 2,
      outputs: ["C:\\Work\\Project.zip"],
      failure: null,
      cancelled: false,
    },
  };
}
