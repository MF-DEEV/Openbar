// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useIsland } from "../island/store";
import { inTauri } from "../shared/bridge";

if (inTauri) {
  void listen<string>("screenshot-preview://open", ({ payload: id }) => {
    if (useIsland.getState().settings.enabled) useIsland.getState().showCapturePreview(id);
  });
  void listen<string>("screenshot-preview://closed", ({ payload: id }) =>
    useIsland.getState().dismissCapturePreview(id),
  );
  useIsland.subscribe((state, previous) => {
    if (previous.capturePreview && previous.capturePreview !== state.capturePreview) {
      // Pin transfers ownership to a floating window; the backend refuses
      // stale island commands, so this cannot destroy that pinned preview.
      void invoke("preview_close", { id: previous.capturePreview }).catch(() => {});
    }
  });
}
