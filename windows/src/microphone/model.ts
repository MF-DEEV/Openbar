// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

export interface MicrophoneSnapshot {
  recording: boolean;
  systemMuted: boolean;
  level: number;
  discord: { muted: boolean; deafened: boolean } | null;
}

export const EMPTY_MICROPHONE: MicrophoneSnapshot = {
  recording: false,
  systemMuted: false,
  level: 0,
  discord: null,
};

export function audioState(snapshot: MicrophoneSnapshot) {
  const deafened = snapshot.discord?.deafened ?? false;
  const muted = deafened || (snapshot.discord?.muted ?? false) || snapshot.systemMuted;
  return {
    muted,
    deafened,
    active: muted,
    level: muted || !Number.isFinite(snapshot.level) ? 0 : Math.max(0, Math.min(1, snapshot.level)),
  };
}
