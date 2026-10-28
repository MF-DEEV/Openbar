// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the tiles and the band ask the backend to do, and the island's part
// in it: a ZIP dialog keeps the island open, and a drag another app took
// can close it (shelfCloseAfterDrop).

import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { S } from "./strings";

let dragging = false;

/** Drags tiles out; nothing else starts while one is in flight. */
export async function dragOut(ids: string[]): Promise<void> {
  if (dragging || ids.length === 0) return;
  dragging = true;
  try {
    const accepted = await bridge.files.dragOut(ids);
    const island = useIsland.getState();
    if (
      accepted &&
      island.settings.shelfCloseAfterDrop &&
      !island.pinned &&
      island.expanded &&
      island.selected === "files" &&
      !island.showingSections
    ) {
      island.collapse();
    }
  } finally {
    dragging = false;
  }
}

/**
 * Create ZIP: the backend asks where to save, above the island, which stays
 * open meanwhile; once a job starts the Files page shows its progress.
 */
export async function createZip(ids: string[]): Promise<void> {
  if (ids.length === 0) return;
  const island = useIsland.getState();
  island.setHeld(true);
  try {
    if (await bridge.files.zip(ids, S.archiveHint)) useIsland.getState().open("files");
  } finally {
    useIsland.getState().setHeld(false);
  }
}
