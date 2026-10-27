// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sample audio for designing Controls and the Mixer in a plain browser (?controls).

import { useAudio } from "../audio/store";
import type { AudioApp, AudioState } from "../audio/types";
import { useBrightness } from "./brightness";
import { useKeepAwake } from "./keepAwake";

const app = (key: string, name: string, volume: number, playing = false, muted = false): AudioApp => ({
  key,
  path: null,
  name,
  volume,
  muted,
  playing,
  system: key === "system",
});

export function demoAudio(): AudioState {
  return {
    supported: true,
    output: { volume: 0.62, muted: false },
    input: { volume: 0.8, muted: false },
    outputs: [
      { id: "speakers", name: "Speakers (Realtek Audio)", isDefault: true },
      { id: "headphones", name: "WH-1000XM5", isDefault: false },
    ],
    inputs: [{ id: "mic", name: "Microphone Array (Realtek Audio)", isDefault: true }],
    canSwitch: true,
    micMuted: false,
    apps: [
      app("c:\\chrome.exe", "Google Chrome", 1, true),
      app("c:\\spotify.exe", "Spotify", 0.45, true),
      app("c:\\discord.exe", "Discord", 1, false, true),
      app("system", "System sounds", 1),
    ],
  };
}

export function fillDemo() {
  useAudio.getState().receive(demoAudio());
  useBrightness.setState({
    loaded: true,
    displays: [{ id: "wmi:panel", name: "Built-in display", brightness: 0.7, builtIn: true }],
  });
  useKeepAwake.setState({ state: { active: true, endsAt: Date.now() + 95 * 60_000 } });
}
