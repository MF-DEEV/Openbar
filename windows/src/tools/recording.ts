// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A recording from the island's side: starts through the capture tools'
// selection, shows in the closed island while it runs, and tells how it ended.

import { create } from "zustand";
import type { Notice } from "../island/store";
import { type RecordingEnded, bridge } from "../shared/bridge";
import { runCapture } from "./capture";
import { S } from "./strings";

export const useRecording = create<{ startedAt: number | null }>(() => ({ startedAt: null }));

export function toggleRecording() {
  if (useRecording.getState().startedAt !== null) void bridge.capture.recorder.stop();
  else void runCapture("recording");
}

export function endedNotice(ended: RecordingEnded): Notice {
  const base = { id: `recording:${Date.now()}`, kind: "capture", priority: 1, duration: 5, opens: "tools" } as const;
  if (!ended.path) return { ...base, title: S.recordingFailed, detail: ended.error ?? undefined };
  const path = ended.path;
  return {
    ...base,
    title: S.recordingSaved,
    detail: ended.missingSound ? S.recordingNoSound : (ended.folder?.split(/[\\/]/).pop() ?? undefined),
    onOpen: () => void bridge.capture.recorder.reveal(path),
  };
}
