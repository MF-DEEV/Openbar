// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";
import { subscribeMicrophone } from "./Microphone";
import type { MicrophoneSnapshot } from "./model";

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it.each([true, false])("briefly announces unmute without a persistent indicator (recording=%s)", async (recording) => {
  vi.useFakeTimers();
  useIsland.setState({ settings: DEFAULT_SETTINGS, notice: null, activities: {} });
  const muted: MicrophoneSnapshot = {
    recording,
    systemMuted: false,
    level: 0,
    discord: { muted: true, deafened: false },
  };
  let receive!: (snapshot: MicrophoneSnapshot) => void;
  vi.spyOn(bridge.microphone, "onSnapshot").mockImplementation(async (handler) => {
    receive = handler;
    return () => {};
  });
  vi.spyOn(bridge.microphone, "snapshot").mockResolvedValue(muted);
  await subscribeMicrophone();
  expect(useIsland.getState().activities.microphone).toBeDefined();

  const unmuted = { ...muted, discord: { muted: false, deafened: false } };
  receive(unmuted);
  const notice = useIsland.getState().notice;
  expect(notice?.detail).toBe("Microphone unmuted");
  expect(notice?.audio?.muted).toBe(false);
  expect(useIsland.getState().activities.microphone).toBeUndefined();

  vi.advanceTimersByTime(2000);
  receive({ ...unmuted, level: 0.7 });
  expect(useIsland.getState().notice).toBe(notice);
  vi.advanceTimersByTime(1000);
  expect(useIsland.getState().notice).toBeNull();
  receive({ ...unmuted, level: 0.4 });
  expect(useIsland.getState().notice).toBeNull();
  expect(useIsland.getState().activities.microphone).toBeUndefined();
});
