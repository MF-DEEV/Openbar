// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires Controls, the Mixer and Keep Awake into the island: their pages,
// the Keep Awake strip, the audio and keep-awake subscriptions, and the
// volume notice for level changes made outside the island (spec 01-shell
// §1.4.3, 05 §1.1: the island's own changes carry its event context and
// raise none).

import { pages } from "../island/views/Expanded";
import { activityViews } from "../island/views/Activity";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { percent, subscribeAudio } from "../audio/store";
import { ControlsLeading, ControlsPage } from "./ControlsPage";
import { KeepAwakeStrip, subscribeKeepAwake } from "./keepAwake";
import { S } from "./strings";
import { MixerLeading, MixerPage, MixerTrailing } from "../mixer/MixerPage";
import "./controls.css";
import "../mixer/mixer.css";

pages.controls = { Page: ControlsPage, Leading: ControlsLeading };
pages.mixer = { Page: MixerPage, Leading: MixerLeading, Trailing: MixerTrailing };
activityViews.keepAwake = KeepAwakeStrip;

void subscribeAudio();
void subscribeKeepAwake();

/** Level notices last 1.6 s at the top priority (NotchSupport.swift:1624-1643). */
void bridge.audio.onVolumeChanged(({ volume, muted }) => {
  const island = useIsland.getState();
  // An open island shows the level in its own slider.
  if (!island.settings.enabled || !island.settings.volumeNotice || island.expanded) return;
  island.showNotice({
    id: `volume:${Date.now()}`,
    kind: "volume",
    title: S.volume,
    detail: muted ? S.muted : `${percent(volume)}%`,
    priority: 3,
    duration: 1.6,
  });
});
