// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { audioState, EMPTY_MICROPHONE } from "./model";

describe("microphone and Discord state", () => {
  it("deafen implies a muted mic even if Discord reports mic unmuted", () => {
    expect(audioState({ ...EMPTY_MICROPHONE, level: 0.8, discord: { muted: false, deafened: true } })).toEqual({
      active: true,
      muted: true,
      deafened: true,
      level: 0,
    });
  });
  it("keeps Discord mute independent of the Windows endpoint", () => {
    expect(
      audioState({ ...EMPTY_MICROPHONE, recording: true, level: 0.8, discord: { muted: true, deafened: false } }).level,
    ).toBe(0);
    expect(
      audioState({
        ...EMPTY_MICROPHONE,
        recording: true,
        systemMuted: true,
        discord: { muted: false, deafened: false },
      }).muted,
    ).toBe(true);
  });
  it("uses actual meter levels and rejects invalid levels", () => {
    expect(audioState({ ...EMPTY_MICROPHONE, recording: true, level: 0.6 }).level).toBe(0.6);
    expect(audioState({ ...EMPTY_MICROPHONE, recording: true, level: NaN }).level).toBe(0);
    expect(audioState(EMPTY_MICROPHONE).active).toBe(false);
    expect(audioState({ ...EMPTY_MICROPHONE, recording: true, level: 0.6 }).active).toBe(false);
  });
});
