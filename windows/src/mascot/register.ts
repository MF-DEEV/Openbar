// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the companion hears from the rest of the island, for the moments no
// section raises a notice for, and Settings asking it to say hello.

import { useAgents } from "../agents/store";
import { countdownStarted, currentCountdown } from "../calendar/store";
import type { Countdown } from "../calendar/model";
import { useDownloads } from "../downloads/Downloads";
import { useBattery } from "../island/battery";
import { useMusic } from "../music/store";
import { bridge } from "../shared/bridge";
import type { BatteryReading } from "../system/types";
import { useTools } from "../tools/store";
import { powerReaction } from "./model";
import { mascotLowPower, onMascotLowPower } from "./power";
import {
  greetMascot,
  mascotHearsMusic,
  mascotSeesAgents,
  reactMascot,
  setMascotKeepAwake,
  setMascotLowPower,
  startMascot as startService,
} from "./service";

/** What the power change `before` → `next` is worth, as NotchService.powerChanged reads it. */
export function batteryReaction(before: BatteryReading, next: BatteryReading) {
  const pluggedIn = !before.pluggedIn && next.pluggedIn;
  const low = (next.percent ?? 100) <= 20 && (before.percent ?? 0) > 20;
  const charged = next.pluggedIn && before.charging && !next.charging && next.percent === 100;
  return powerReaction(pluggedIn, charged, low);
}

/** The first time the battery is read is no news, nor is one that comes or goes. */
function followBattery() {
  useBattery.subscribe((s, previous) => {
    if (!s.battery || !previous.battery || s.battery === previous.battery) return;
    const reaction = batteryReaction(previous.battery, s.battery);
    if (reaction) reactMascot(reaction);
  });
}

function followDownloads() {
  const status = new Map(useDownloads.getState().items.map((item) => [item.id, item.status]));
  useDownloads.subscribe((s) => {
    for (const item of s.items) {
      const before = status.get(item.id);
      status.set(item.id, item.status);
      if (before !== "downloading") continue;
      if (item.status === "completed") reactMascot("celebrate");
      else if (item.status === "interrupted") reactMascot("confused");
    }
  });
}

function followCalendar() {
  let saw: Countdown | null = null;
  const look = () => {
    const next = currentCountdown();
    if (countdownStarted(saw, next, Date.now())) reactMascot("bounce");
    saw = next;
  };
  look();
  // The countdown reads the clock, which moves with nothing else to tell it.
  setInterval(look, 15_000);
}

let started = false;

export function startMascot() {
  if (started) return;
  started = true;
  startService();
  followBattery();
  followDownloads();
  followCalendar();

  useMusic.subscribe((s, previous) => {
    if (s.playback?.isPlaying && !previous.playback?.isPlaying) mascotHearsMusic();
  });
  const agents = () => {
    const { snapshot } = useAgents.getState();
    // Agents already at work when the app opens are no news: their logs are read after it shows.
    if (snapshot.loaded) mascotSeesAgents(snapshot.live.length > 0);
  };
  useAgents.subscribe(agents);
  agents();
  setMascotKeepAwake(useTools.getState().keepAwake);
  useTools.subscribe((s) => setMascotKeepAwake(s.keepAwake));
  setMascotLowPower(mascotLowPower());
  onMascotLowPower(() => setMascotLowPower(mascotLowPower()));
  void bridge.session.onUnlock(() => reactMascot("wakeUp"));
  void bridge.mascot.onGreet(() => greetMascot());
}
