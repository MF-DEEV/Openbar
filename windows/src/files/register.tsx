// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Puts the Files section in the island: its page and band, the shelf's
// events, and what a drag over the island does (NotchService.beginFileDrop,
// fileDragChanged, accept; spec 05 §6.2). A drag entering the closed island
// shows where to drop (Settings › Content › Files › Show a drop target), or
// opens Files when that is off; one entering the open island turns it to
// Files. Dropping opens Files without taking the keyboard.

import { modulesOf, useIsland } from "../island/store";
import { pages } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import { FilesLeading, FilesPage, FilesTrailing } from "./FilesPage";
import { useFiles } from "./store";
import { S } from "./strings";
import type { DragPhase } from "./types";

pages.files = { Page: FilesPage, Leading: FilesLeading, Trailing: FilesTrailing };

const shown = () => modulesOf(useIsland.getState()).includes("files");

export function handleDrag(drag: DragPhase) {
  const island = useIsland.getState();
  if (drag.phase === "leave") {
    island.setDropHint(false);
    return;
  }
  if (drag.phase === "drop") {
    island.setDropHint(false);
    if (drag.added && shown()) island.open("files", { takeFocus: false });
    return;
  }
  if (!shown()) return;
  if (island.expanded) {
    if (island.selected !== "files" || island.showingSections) island.open("files", { takeFocus: false });
  } else if (island.settings.dragReveal) {
    island.setDropHint(true);
  } else {
    island.open("files", { takeFocus: false });
  }
}

/** Listens first, so the first snapshot can't overwrite a newer event. */
export async function subscribeFiles() {
  let version = 0;
  await bridge.files.onShelf((snapshot) => {
    version++;
    useFiles.getState().receive(snapshot);
  });
  await bridge.files.onZip((zip) => useFiles.getState().receiveZip(zip));
  await bridge.files.onDrag(handleDrag);
  await bridge.files.onMissing(() =>
    useIsland.getState().showNotice({
      id: `files-missing:${Date.now()}`,
      kind: "files",
      title: S.fileMissing,
      priority: 0,
      duration: 3,
      opens: "files",
    }),
  );
  const at = version;
  const first = await bridge.files.snapshot();
  if (first && version === at) useFiles.getState().receive(first);
}

void subscribeFiles();

// The section switched off mid-drag takes its hint with it.
useIsland.subscribe((s, previous) => {
  if (s.settings !== previous.settings && s.dropHint && !shown()) s.setDropHint(false);
});
